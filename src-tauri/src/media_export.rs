use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use comrak::{markdown_to_html, Options};

use crate::fs_atomic::atomic_write_bytes;
use crate::fs_atomic::atomic_write_str;
use crate::links::{replace_wiki_links_with_display, split_yaml_frontmatter};

/// Formats epoch milliseconds as `Pasted image YYYYMMDD-HHMMSS.png`.
pub fn format_pasted_image_filename(ms: i64) -> String {
    let total_secs = ms.div_euclid(1000);
    let days = total_secs.div_euclid(86_400);
    let day_secs = total_secs.rem_euclid(86_400);

    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("Pasted image {year:04}{m:02}{d:02}-{hours:02}{minutes:02}{seconds:02}.png")
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xedb8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in bytes {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let checksum = crc32(&out[crc_start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

/// Encodes raw 8-bit RGBA pixels into a valid standard PNG byte buffer.
pub fn encode_rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "Image dimensions overflow".to_string())?;
    if width == 0 || height == 0 || rgba.len() < expected_len {
        return Err("Invalid RGBA buffer length for PNG encoding".to_string());
    }

    let row_bytes = (width as usize) * 4;
    let mut raw_scanlines = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw_scanlines.push(0u8); // Filter type 0 (None)
        let start = y * row_bytes;
        raw_scanlines.extend_from_slice(&rgba[start..start + row_bytes]);
    }

    // Wrap scanlines in a valid zlib stream using uncompressed DEFLATE blocks (max 65535 bytes per block)
    let mut zlib_data = Vec::with_capacity(raw_scanlines.len() + 64);
    zlib_data.push(0x78); // CMF (Deflate, 32K window)
    zlib_data.push(0x01); // FLG (No preset dict, check bits for 0x7801)

    let chunks: Vec<&[u8]> = raw_scanlines.chunks(65_535).collect();
    for (idx, chunk) in chunks.iter().enumerate() {
        let is_final = idx + 1 == chunks.len();
        zlib_data.push(if is_final { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        let nlen = !len;
        zlib_data.extend_from_slice(&len.to_le_bytes());
        zlib_data.extend_from_slice(&nlen.to_le_bytes());
        zlib_data.extend_from_slice(chunk);
    }
    zlib_data.extend_from_slice(&adler32(&raw_scanlines).to_be_bytes());

    let mut png = Vec::with_capacity(zlib_data.len() + 64);
    // PNG Magic Signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // Bit depth: 8
    ihdr.push(6); // Color type: 6 (RGBA)
    ihdr.push(0); // Compression: 0
    ihdr.push(0); // Filter: 0
    ihdr.push(0); // Interlace: 0
    write_png_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk
    write_png_chunk(&mut png, b"IDAT", &zlib_data);

    // IEND chunk
    write_png_chunk(&mut png, b"IEND", &[]);

    Ok(png)
}

/// Saves PNG bytes into `<vault_root>/attachments/Pasted image YYYYMMDD-HHMMSS.png`
/// and returns `(file_name, rel_path)` (e.g. `("Pasted image 20261019-014000.png", "attachments/Pasted image 20261019-014000.png")`).
pub fn save_png_to_vault_attachments(
    vault_root: &Path,
    png_bytes: &[u8],
    timestamp_ms: Option<i64>,
) -> Result<(String, String), String> {
    let attachments_dir = vault_root.join("attachments");
    crate::vault::ensure_vault_containment(vault_root, &attachments_dir)?;
    fs::create_dir_all(&attachments_dir)
        .map_err(|e| format!("Failed to create attachments folder: {e}"))?;

    let ms = timestamp_ms.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });
    let base_name = format_pasted_image_filename(ms);
    let stem = base_name.strip_suffix(".png").unwrap_or(&base_name);

    let mut final_name = base_name.clone();
    let mut target_path = attachments_dir.join(&final_name);
    let mut counter = 1u32;
    while target_path.exists() {
        final_name = format!("{stem}-{counter}.png");
        target_path = attachments_dir.join(&final_name);
        counter += 1;
    }

    atomic_write_bytes(&target_path, png_bytes)?;
    let rel_path = format!("attachments/{final_name}");
    Ok((final_name, rel_path))
}

/// Reads an image from the system clipboard via `arboard` and saves it as a PNG in `<vault_root>/attachments`.
pub fn paste_clipboard_image_to_vault(vault_root: &Path) -> Result<(String, String), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to open system clipboard: {e}"))?;
    let image = clipboard
        .get_image()
        .map_err(|e| format!("No image found on system clipboard: {e}"))?;
    let png_bytes = encode_rgba_to_png(
        image.width as u32,
        image.height as u32,
        image.bytes.as_ref(),
    )?;
    save_png_to_vault_attachments(vault_root, &png_bytes, None)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn percent_decode_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_safe_relative_subpath(rel: &str) -> bool {
    let path = Path::new(rel);
    path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn resolve_vault_image_path(vault_root: &Path, target: &str) -> Option<PathBuf> {
    let decoded = percent_decode_path(target.trim())
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string();
    if decoded.is_empty() || !is_safe_relative_subpath(&decoded) {
        return None;
    }

    // 1. Direct relative path inside vault_root
    let direct = vault_root.join(&decoded);
    if crate::vault::ensure_vault_containment(vault_root, &direct).is_ok() && direct.is_file() {
        return Some(direct);
    }

    // 2. Inside `<vault_root>/attachments/<target>`
    let in_attachments = vault_root.join("attachments").join(&decoded);
    if crate::vault::ensure_vault_containment(vault_root, &in_attachments).is_ok()
        && in_attachments.is_file()
    {
        return Some(in_attachments);
    }

    None
}

fn image_mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Resolves an image reference (`![[Pasted image ...png]]` or `![](attachments/...)`) inside `vault_root`
/// and returns a `data:image/...;base64,...` URL for Live Preview and HTML export.
pub fn read_vault_image_as_data_url(vault_root: &Path, raw_target: &str) -> Result<String, String> {
    let pipe_trimmed = raw_target.split('|').next().unwrap_or(raw_target).trim();
    let path = resolve_vault_image_path(vault_root, pipe_trimmed)
        .ok_or_else(|| format!("Attachment not found in vault: {pipe_trimmed}"))?;
    let bytes = fs::read(&path)
        .map_err(|e| format!("Failed to read attachment {}: {e}", path.display()))?;
    let mime = image_mime_for_path(&path);
    let encoded = base64_encode(&bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn preprocess_embeds_and_wikilinks(markdown: &str, vault_root: Option<&Path>) -> String {
    let (body, _) = split_yaml_frontmatter(markdown);
    let mut out_lines = Vec::new();
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out_lines.push(line.to_string());
            continue;
        }
        if in_fence {
            out_lines.push(line.to_string());
            continue;
        }

        // Convert Obsidian image embeds `![[image.png]]` to standard Markdown `![image.png](data_url_or_path)`
        let mut converted = String::with_capacity(line.len());
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if i + 2 < chars.len() && chars[i] == '!' && chars[i + 1] == '[' && chars[i + 2] == '['
            {
                let mut close_idx = None;
                let mut j = i + 3;
                while j + 1 < chars.len() {
                    if chars[j] == ']' && chars[j + 1] == ']' {
                        close_idx = Some(j);
                        break;
                    }
                    j += 1;
                }
                if let Some(end) = close_idx {
                    let inner: String = chars[i + 3..end].iter().collect();
                    let target = inner.split('|').next().unwrap_or(&inner).trim();
                    let src = vault_root
                        .and_then(|root| read_vault_image_as_data_url(root, target).ok())
                        .unwrap_or_else(|| target.to_string());
                    converted.push_str(&format!("![{target}]({src})"));
                    i = end + 2;
                    continue;
                }
            }
            converted.push(chars[i]);
            i += 1;
        }

        let with_links = replace_wiki_links_with_display(&converted);
        out_lines.push(with_links);
    }

    out_lines.join("\n")
}

/// Builds a self-contained HTML document with Jotter's Fluent theme CSS (supporting light, dark, and print)
/// and rendered Markdown body.
pub fn build_self_contained_html(
    title: &str,
    markdown: &str,
    vault_root: Option<&Path>,
    dark_mode: bool,
) -> String {
    let preprocessed = preprocess_embeds_and_wikilinks(markdown, vault_root);

    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    // Emits `<span data-math-style>` for `$…$` and `$$…$$`; the frontend renders them with KaTeX.
    options.extension.math_dollars = true;
    options.render.unsafe_ = false;

    let mut html_body = markdown_to_html(&preprocessed, &options);

    // Inline any relative `<img src="...">` paths from the vault so the HTML file is 100% self-contained
    if let Some(root) = vault_root {
        let mut cursor = 0;
        while let Some(pos) = html_body[cursor..].find("<img src=\"") {
            let src_start = cursor + pos + 10;
            if let Some(end_offset) = html_body[src_start..].find('"') {
                let src_end = src_start + end_offset;
                let src_val = &html_body[src_start..src_end];
                if !src_val.starts_with("data:")
                    && !src_val.starts_with("http://")
                    && !src_val.starts_with("https://")
                {
                    if let Ok(data_url) = read_vault_image_as_data_url(root, src_val) {
                        html_body.replace_range(src_start..src_end, &data_url);
                        cursor = src_start + data_url.len() + 1;
                        continue;
                    }
                }
                cursor = src_end + 1;
            } else {
                break;
            }
        }
    }

    let escaped_title = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let theme_attr = if dark_mode { "dark" } else { "light" };

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="{theme_attr}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{escaped_title}</title>
  <style>
    :root {{
      --font-ui: 'Segoe UI Variable Text', 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif;
      --font-display: 'Segoe UI Variable Display', 'Segoe UI Variable', 'Segoe UI', system-ui, sans-serif;
      --font-mono: 'Cascadia Code', 'Consolas', ui-monospace, monospace;
      --bg-page: #f3f3f3;
      --bg-card: #ffffff;
      --bg-code: rgba(0, 0, 0, 0.05);
      --text-primary: #1b1b1b;
      --text-secondary: #5c5c5c;
      --border-subtle: rgba(0, 0, 0, 0.12);
      --accent: #0067c0;
      --syn-heading: #005fb8;
      --syn-code: #9a3412;
    }}
    html[data-theme="dark"] {{
      --bg-page: #202020;
      --bg-card: #282828;
      --bg-code: rgba(255, 255, 255, 0.08);
      --text-primary: #f5f5f5;
      --text-secondary: #c2c2c2;
      --border-subtle: rgba(255, 255, 255, 0.14);
      --accent: #4cc2ff;
      --syn-heading: #60cdff;
      --syn-code: #f59e0b;
    }}
    @media print {{
      :root, html[data-theme="dark"] {{
        --bg-page: #ffffff;
        --bg-card: #ffffff;
        --bg-code: #f5f5f5;
        --text-primary: #000000;
        --text-secondary: #333333;
        --border-subtle: #cccccc;
        --accent: #005fb8;
        --syn-heading: #000000;
        --syn-code: #333333;
      }}
      body {{
        padding: 0 !important;
        background: #ffffff !important;
      }}
      main.jotter-exported-note {{
        border: none !important;
        padding: 0 !important;
        max-width: 100% !important;
      }}
    }}
    * {{
      box-sizing: border-box;
    }}
    body {{
      margin: 0;
      padding: 32px 16px;
      font-family: var(--font-ui);
      font-size: 16px;
      line-height: 1.65;
      background: var(--bg-page);
      color: var(--text-primary);
    }}
    main.jotter-exported-note {{
      max-width: 720px;
      margin: 0 auto;
      background: var(--bg-card);
      border: 1px solid var(--border-subtle);
      border-radius: 8px;
      padding: 32px 40px;
    }}
    h1, h2, h3, h4, h5, h6 {{
      font-family: var(--font-display);
      color: var(--syn-heading);
      line-height: 1.25;
      margin-top: 1.4em;
      margin-bottom: 0.5em;
    }}
    h1 {{ font-size: 28px; margin-top: 0; }}
    h2 {{ font-size: 20px; }}
    h3 {{ font-size: 16px; }}
    p, ul, ol, blockquote, pre, table {{
      margin-top: 0;
      margin-bottom: 1em;
    }}
    code {{
      font-family: var(--font-mono);
      font-size: 14px;
      background: var(--bg-code);
      color: var(--syn-code);
      padding: 2px 5px;
      border-radius: 4px;
    }}
    pre {{
      background: var(--bg-code);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      padding: 12px 16px;
      overflow-x: auto;
    }}
    pre code {{
      background: transparent;
      padding: 0;
      color: var(--text-primary);
    }}
    blockquote {{
      border-left: 3px solid var(--accent);
      margin-left: 0;
      padding: 4px 0 4px 16px;
      color: var(--text-secondary);
      font-style: italic;
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 14px;
    }}
    th, td {{
      border: 1px solid var(--border-subtle);
      padding: 8px 12px;
      text-align: left;
    }}
    th {{
      background: var(--bg-code);
      font-weight: 600;
    }}
    img {{
      max-width: 100%;
      height: auto;
      border-radius: 6px;
    }}
    a {{
      color: var(--accent);
    }}
  </style>
</head>
<body>
  <main class="jotter-exported-note">
{html_body}  </main>
</body>
</html>
"#
    )
}

/// Writes an HTML export, built by `build_self_contained_html` and finished by the frontend,
/// to `target_path`.
pub fn export_html_to_path(target_path: &Path, html: &str) -> Result<String, String> {
    atomic_write_str(target_path, html)?;
    Ok(target_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_rgba_to_valid_png_and_saves_to_attachments_folder() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        // 2x2 RGBA image (16 bytes)
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let png = encode_rgba_to_png(2, 2, &rgba).expect("encode png");
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));

        // Save at fixed timestamp 2026-10-19T01:40:00Z (1_792_374_000_000 ms)
        let (name, rel) =
            save_png_to_vault_attachments(root, &png, Some(1_792_374_000_000)).unwrap();
        assert_eq!(name, "Pasted image 20261019-014000.png");
        assert_eq!(rel, "attachments/Pasted image 20261019-014000.png");
        assert!(root.join(&rel).is_file());

        // Read back as data URL via wiki-link target name
        let data_url = read_vault_image_as_data_url(root, &name).unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn renders_footnotes_and_leaves_callout_markers_for_the_frontend() {
        let html = build_self_contained_html(
            "Notes",
            "Claim[^1].

> [!note] Heads up
> body

[^1]: The source.",
            None,
            false,
        );
        assert!(html.contains("data-footnotes"));
        assert!(html.contains("The source."));
        assert!(html.contains("[!note] Heads up"));
    }

    #[test]
    fn builds_self_contained_html_with_theme_css_and_inlined_attachments() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        let rgba = [0, 120, 212, 255];
        let png = encode_rgba_to_png(1, 1, &rgba).unwrap();
        let (img_name, _) =
            save_png_to_vault_attachments(root, &png, Some(1_792_374_000_000)).unwrap();

        let md = format!(
            "---\ntags: [export]\n---\n# Export Test\n\nParagraph with **bold** and ![[{img_name}]]."
        );
        let html = build_self_contained_html("Export Test", &md, Some(root), false);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("--font-ui:"));
        assert!(html.contains("<h1>Export Test</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("data:image/png;base64,"));
    }

    #[test]
    fn marks_math_and_keeps_mermaid_blocks_for_the_frontend() {
        let md = r"Area is $\pi r^2$, costs $5 and $10.

$$
E = mc^2
$$

```mermaid
flowchart LR
  A --> B
```
";
        let html = build_self_contained_html("Math", md, None, false);
        assert!(
            html.contains(r#"<span data-math-style="inline">\pi r^2</span>"#),
            "{html}"
        );
        assert!(
            html.contains("<span data-math-style=\"display\">"),
            "{html}"
        );
        assert!(html.contains("costs $5 and $10"), "{html}");
        assert!(html.contains("<code class=\"language-mermaid\">"), "{html}");
    }
}
