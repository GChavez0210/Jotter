use comrak::{markdown_to_html, Options};

use crate::links::replace_wiki_links_with_display;
use crate::types::{ClipboardPayload, CopyMode};

/// Normalizes all Windows (`\r\n`) and legacy Mac (`\r`) line endings to LF (`\n`),
/// preserving exact line break counts (including leading, trailing, and consecutive blank lines).
pub fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

/// Escapes HTML special characters so plain text or raw Markdown can be safely embedded
/// inside a `text/html` clipboard payload.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escapes HTML special characters on a single line while converting:
/// - Leading spaces to `&nbsp;`
/// - Repeated spaces (2nd and subsequent spaces in a run) to `&nbsp;`
/// - Every tab (`\t`) to four `&nbsp;` (`&nbsp;&nbsp;&nbsp;&nbsp;`)
fn escape_line_with_nbsp(line: &str) -> String {
    let mut out = String::with_capacity(line.len() * 2);
    let mut in_leading = true;
    let mut prev_was_space = false;

    for ch in line.chars() {
        match ch {
            '\t' => {
                out.push_str("&nbsp;&nbsp;&nbsp;&nbsp;");
                prev_was_space = true;
            }
            ' ' => {
                if in_leading || prev_was_space {
                    out.push_str("&nbsp;");
                } else {
                    out.push(' ');
                }
                prev_was_space = true;
            }
            '&' => {
                in_leading = false;
                prev_was_space = false;
                out.push_str("&amp;");
            }
            '<' => {
                in_leading = false;
                prev_was_space = false;
                out.push_str("&lt;");
            }
            '>' => {
                in_leading = false;
                prev_was_space = false;
                out.push_str("&gt;");
            }
            '"' => {
                in_leading = false;
                prev_was_space = false;
                out.push_str("&quot;");
            }
            '\'' => {
                in_leading = false;
                prev_was_space = false;
                out.push_str("&#39;");
            }
            _ => {
                in_leading = false;
                prev_was_space = false;
                out.push(ch);
            }
        }
    }

    out
}

/// Converts normalized text (`\n` line breaks) into HTML where:
/// - Every `\n` becomes `<br>`
/// - Leading and repeated spaces become `&nbsp;`
/// - Every tab (`\t`) becomes four `&nbsp;`
///
/// Used by both `MarkdownSource` and `Rich` modes so indentation survives in HTML targets.
pub fn plain_to_br_html(plain_normalized: &str) -> String {
    plain_normalized
        .split('\n')
        .map(escape_line_with_nbsp)
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Converts tabs and repeated spaces inside an already-rendered HTML fragment (outside `<...>` tags)
/// into `&nbsp;` so mid-line multi-space runs and tabs survive in `Rich` mode.
fn preserve_repeated_spaces_in_rendered_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 16);
    let mut in_tag = false;
    let mut prev_was_space = false;

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                prev_was_space = false;
                out.push('<');
            }
            '>' => {
                in_tag = false;
                prev_was_space = false;
                out.push('>');
            }
            '\t' if !in_tag => {
                out.push_str("&nbsp;&nbsp;&nbsp;&nbsp;");
                prev_was_space = true;
            }
            ' ' if !in_tag => {
                if prev_was_space {
                    out.push_str("&nbsp;");
                } else {
                    out.push(' ');
                }
                prev_was_space = true;
            }
            _ => {
                if !in_tag {
                    prev_was_space = false;
                }
                out.push(ch);
            }
        }
    }

    out
}

/// Strips inline Markdown formatting (`**bold**`, `*italic*`, `~~strike~~`, `` `code` ``,
/// `[label](url)`, `![alt](url)`, `[[wiki|alias]]`) from a single line.
/// Wiki links inside inline code spans (`` `[[wiki]]` ``) are not rewritten.
fn strip_inline_markdown(line: &str) -> String {
    let with_wiki = replace_wiki_links_with_display(line);
    let chars: Vec<char> = with_wiki.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        // Escaped character: `\*`, `\_`, etc.
        if chars[i] == '\\' && i + 1 < len && chars[i + 1].is_ascii_punctuation() {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }

        // Inline code span: `code` or ``code``
        if chars[i] == '`' {
            let mut tick_count = 0;
            while i + tick_count < len && chars[i + tick_count] == '`' {
                tick_count += 1;
            }
            let content_start = i + tick_count;
            let mut j = content_start;
            let mut found_close = None;
            while j + tick_count <= len {
                if (0..tick_count).all(|k| chars[j + k] == '`') {
                    found_close = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(close_idx) = found_close {
                for &c in &chars[content_start..close_idx] {
                    out.push(c);
                }
                i = close_idx + tick_count;
                continue;
            }
        }

        // Image `![alt](url)` or link `[label](url)`
        if (chars[i] == '!' && i + 1 < len && chars[i + 1] == '[') || chars[i] == '[' {
            let bracket_start = if chars[i] == '!' { i + 1 } else { i };
            let mut close_bracket = None;
            let mut k = bracket_start + 1;
            while k < len {
                if chars[k] == ']' {
                    close_bracket = Some(k);
                    break;
                }
                k += 1;
            }
            if let Some(cb) = close_bracket {
                if cb + 1 < len && chars[cb + 1] == '(' {
                    let mut close_paren = None;
                    let mut p = cb + 2;
                    while p < len {
                        if chars[p] == ')' {
                            close_paren = Some(p);
                            break;
                        }
                        p += 1;
                    }
                    if let Some(cp) = close_paren {
                        let label: String = chars[bracket_start + 1..cb].iter().collect();
                        out.push_str(&strip_inline_markdown(&label));
                        i = cp + 1;
                        continue;
                    }
                }
            }
        }

        // Bold / strikethrough markers: `**`, `__`, `~~`
        if i + 1 < len
            && ((chars[i] == '*' && chars[i + 1] == '*')
                || (chars[i] == '_' && chars[i + 1] == '_')
                || (chars[i] == '~' && chars[i + 1] == '~'))
        {
            let marker = chars[i];
            let mut close_pos = None;
            let mut k = i + 2;
            while k + 1 < len {
                if chars[k] == marker && chars[k + 1] == marker {
                    close_pos = Some(k);
                    break;
                }
                k += 1;
            }
            if let Some(cp) = close_pos {
                let inner: String = chars[i + 2..cp].iter().collect();
                out.push_str(&strip_inline_markdown(&inner));
                i = cp + 2;
                continue;
            }
        }

        // Single emphasis marker: `*text*` or `_text_`
        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            let prev_is_alnum = i > 0 && chars[i - 1].is_alphanumeric();
            let next_non_space = i + 1 < len && !chars[i + 1].is_whitespace();
            if !prev_is_alnum && next_non_space {
                let mut close_pos = None;
                let mut k = i + 1;
                while k < len {
                    if chars[k] == marker && !chars[k - 1].is_whitespace() {
                        close_pos = Some(k);
                        break;
                    }
                    k += 1;
                }
                if let Some(cp) = close_pos {
                    let inner: String = chars[i + 1..cp].iter().collect();
                    out.push_str(&strip_inline_markdown(&inner));
                    i = cp + 1;
                    continue;
                }
            }
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// Strips block and inline Markdown syntax line-by-line while preserving
/// every single `\n` newline 1-to-1. Code fence lines (` ``` ` and `~~~`) are dropped
/// entirely so the line count matches the code block content.
pub fn strip_markdown_to_plain(markdown: &str) -> String {
    let normalized = normalize_newlines(markdown);
    let mut in_fence = false;
    let mut out_lines = Vec::new();

    for line in normalized.split('\n') {
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out_lines.push(line.to_string());
            continue;
        }

        // Strip ATX heading prefix (`# `, `## `, ..., `###### `)
        let mut rest = trimmed_start;
        let hash_count = rest.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hash_count) && rest[hash_count..].starts_with(' ') {
            rest = rest[hash_count + 1..].trim_start();
            out_lines.push(strip_inline_markdown(rest));
            continue;
        }

        // Strip blockquote prefix (`> ` or `>`)
        if let Some(after_quote) = trimmed_start.strip_prefix('>') {
            let quote_body = after_quote.strip_prefix(' ').unwrap_or(after_quote);
            out_lines.push(strip_inline_markdown(quote_body));
            continue;
        }

        // Preserve leading indentation for list items while stripping task list / inline syntax
        let leading_ws_len = line.len() - trimmed_start.len();
        let leading_ws = &line[..leading_ws_len];

        for prefix in ["- [ ] ", "* [ ] ", "+ [ ] "] {
            if let Some(after_task) = trimmed_start.strip_prefix(prefix) {
                out_lines.push(format!("{leading_ws}[ ] {}", strip_inline_markdown(after_task)));
                rest = "";
                break;
            }
        }
        if rest.is_empty() && !trimmed_start.is_empty() {
            continue;
        }

        for prefix in ["- [x] ", "- [X] ", "* [x] ", "* [X] ", "+ [x] ", "+ [X] "] {
            if let Some(after_task) = trimmed_start.strip_prefix(prefix) {
                out_lines.push(format!("{leading_ws}[x] {}", strip_inline_markdown(after_task)));
                rest = "";
                break;
            }
        }
        if rest.is_empty() && !trimmed_start.is_empty() {
            continue;
        }

        out_lines.push(strip_inline_markdown(line));
    }

    out_lines.join("\n")
}

/// Renders a single line of inline Markdown using `comrak` and strips the outer `<p>...</p>` wrapper
/// so lines can be joined strictly with `<br>` without paragraph spacing.
fn render_inline_line_with_comrak(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }

    let with_wiki = replace_wiki_links_with_display(line);
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.table = true;

    let rendered = markdown_to_html(&with_wiki, &options);
    let trimmed = rendered.trim_end_matches('\n').trim();
    let unwrapped = if let Some(inner) = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        inner.to_string()
    } else {
        trimmed.replace('\n', "<br>")
    };

    preserve_repeated_spaces_in_rendered_html(&unwrapped)
}

/// Converts Markdown to rich HTML via `comrak` while ensuring:
/// - Every `\n` in the output becomes `<br>`
/// - Code fence lines (` ``` ` and `~~~`) are dropped entirely
/// - Leading indentation, repeated spaces, and tabs are converted to `&nbsp;` via `plain_to_br_html`
pub fn markdown_to_br_html(markdown: &str) -> String {
    let normalized = normalize_newlines(markdown);
    let mut in_fence = false;
    let mut html_lines = Vec::new();

    for line in normalized.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            html_lines.push(format!("<code>{}</code>", plain_to_br_html(line)));
            continue;
        }

        let leading_ws_len = line.len() - trimmed.len();
        let leading_html = if leading_ws_len > 0 {
            plain_to_br_html(&line[..leading_ws_len])
        } else {
            String::new()
        };

        // Heading (`# Heading` -> `<strong>Heading</strong>`)
        let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hash_count) && trimmed[hash_count..].starts_with(' ') {
            let heading_text = trimmed[hash_count + 1..].trim_start();
            html_lines.push(format!(
                "{leading_html}<strong>{}</strong>",
                render_inline_line_with_comrak(heading_text)
            ));
            continue;
        }

        // Task list item
        let mut handled_task = false;
        for prefix in ["- [ ] ", "* [ ] ", "+ [ ] "] {
            if let Some(after_task) = trimmed.strip_prefix(prefix) {
                html_lines.push(format!(
                    "{leading_html}&#9744; {}",
                    render_inline_line_with_comrak(after_task)
                ));
                handled_task = true;
                break;
            }
        }
        if handled_task {
            continue;
        }

        for prefix in ["- [x] ", "- [X] ", "* [x] ", "* [X] ", "+ [x] ", "+ [X] "] {
            if let Some(after_task) = trimmed.strip_prefix(prefix) {
                html_lines.push(format!(
                    "{leading_html}&#9745; {}",
                    render_inline_line_with_comrak(after_task)
                ));
                handled_task = true;
                break;
            }
        }
        if handled_task {
            continue;
        }

        // Unordered list item (`- item`, `* item`, `+ item`)
        if let Some(after_bullet) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            html_lines.push(format!(
                "{leading_html}&bull; {}",
                render_inline_line_with_comrak(after_bullet)
            ));
            continue;
        }

        // Blockquote (`> quote`)
        if let Some(after_quote) = trimmed.strip_prefix('>') {
            let quote_body = after_quote.strip_prefix(' ').unwrap_or(after_quote);
            html_lines.push(format!(
                "{leading_html}<em>{}</em>",
                render_inline_line_with_comrak(quote_body)
            ));
            continue;
        }

        if trimmed.is_empty() {
            html_lines.push(leading_html);
        } else {
            html_lines.push(format!(
                "{leading_html}{}",
                render_inline_line_with_comrak(trimmed)
            ));
        }
    }

    html_lines.join("<br>")
}

/// Builds the clipboard payload for the requested `CopyMode`.
///
/// - `MarkdownSource` (default): preserves raw Markdown syntax in `text/plain` (`\n` line breaks)
///   and HTML-escaped `text/html` (`<br>` line breaks and `&nbsp;` indentation).
/// - `Plain`: strips Markdown formatting to clean plain text with `\n` line breaks only (`html` is empty).
/// - `Rich`: renders inline/block Markdown via `comrak` into `text/html` where every `\n` is `<br>`
///   and indentation uses `&nbsp;`, paired with the stripped plain-text alternative (`\n` line breaks).
pub fn build_clipboard_payload(text: &str, mode: CopyMode) -> ClipboardPayload {
    let normalized = normalize_newlines(text);

    let (plain, html) = match mode {
        CopyMode::Plain => {
            let stripped = strip_markdown_to_plain(&normalized);
            (stripped, String::new())
        }
        CopyMode::Rich => {
            let stripped = strip_markdown_to_plain(&normalized);
            let rich_html = markdown_to_br_html(&normalized);
            (stripped, rich_html)
        }
        CopyMode::MarkdownSource => {
            let br_html = plain_to_br_html(&normalized);
            (normalized, br_html)
        }
    };

    let line_count = if plain.is_empty() {
        0
    } else {
        plain.split('\n').count()
    };
    let char_count = plain.chars().count();

    ClipboardPayload {
        plain,
        html,
        mode,
        line_count,
        char_count,
    }
}

/// Writes the prepared payload to the system clipboard using `arboard`:
/// - `CopyMode::Plain`: writes `text/plain` only via `arboard::Clipboard::set_text` (no `text/html`).
/// - `CopyMode::Rich` and `CopyMode::MarkdownSource`: writes both `text/html` and `text/plain`
///   via `arboard::Clipboard::set_html`.
pub fn write_to_system_clipboard(payload: &ClipboardPayload) -> Result<(), String> {
    let mut last_err = String::new();
    for _ in 0..5 {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                let res = match payload.mode {
                    CopyMode::Plain => clipboard
                        .set_text(&payload.plain)
                        .map_err(|e| format!("Failed to write plain text to clipboard: {e}")),
                    CopyMode::Rich | CopyMode::MarkdownSource => clipboard
                        .set_html(&payload.html, Some(&payload.plain))
                        .map_err(|e| {
                            format!("Failed to write HTML and plain text to clipboard: {e}")
                        }),
                };
                match res {
                    Ok(()) => return Ok(()),
                    Err(e) => last_err = e,
                }
            }
            Err(e) => {
                last_err = format!("Failed to open system clipboard: {e}");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    Err(last_err)
}

/// Reads plain text from the system clipboard via `arboard`, normalizing line endings to `\n`
/// for the `Ctrl+Shift+V` paste-as-plain-text command.
pub fn read_plain_from_system_clipboard() -> Result<String, String> {
    let mut last_err = String::new();
    for _ in 0..5 {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => match clipboard.get_text() {
                Ok(raw) => return Ok(normalize_newlines(&raw)),
                Err(e) => last_err = format!("Failed to read plain text from clipboard: {e}"),
            },
            Err(e) => last_err = format!("Failed to open system clipboard: {e}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    Err(last_err)
}

/// Reads the raw `CF_HTML` (`HTML Format`) payload from the Windows system clipboard.
/// Returns `Err` if `CF_HTML` is not currently present on the clipboard.
#[cfg(target_os = "windows")]
pub fn read_html_from_system_clipboard() -> Result<String, String> {
    use std::ffi::c_void;
    use std::ptr;

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClipboardFormatW(lpsz_format: *const u16) -> u32;
        fn OpenClipboard(hwnd_new_owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn GetClipboardData(u_format: u32) -> *mut c_void;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalLock(h_mem: *mut c_void) -> *const u8;
        fn GlobalUnlock(h_mem: *mut c_void) -> i32;
        fn GlobalSize(h_mem: *mut c_void) -> usize;
    }

    let format_name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
    // SAFETY: Standard Win32 clipboard read sequence with proper lock/unlock and close.
    unsafe {
        let cf_html = RegisterClipboardFormatW(format_name.as_ptr());
        if cf_html == 0 {
            return Err("RegisterClipboardFormatW failed for HTML Format".to_string());
        }

        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return Err("OpenClipboard failed".to_string());
        }

        let result = if IsClipboardFormatAvailable(cf_html) == 0 {
            Err("CF_HTML (HTML Format) is not available on the clipboard".to_string())
        } else {
            let handle = GetClipboardData(cf_html);
            if handle.is_null() {
                Err("GetClipboardData(CF_HTML) returned null".to_string())
            } else {
                let ptr = GlobalLock(handle);
                if ptr.is_null() {
                    Err("GlobalLock(CF_HTML) returned null".to_string())
                } else {
                    let size = GlobalSize(handle);
                    let bytes = std::slice::from_raw_parts(ptr, size);
                    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                    let text = String::from_utf8_lossy(&bytes[..end]).into_owned();
                    let _ = GlobalUnlock(handle);
                    Ok(text)
                }
            }
        };

        let _ = CloseClipboard();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_copy_mode_is_markdown_source() {
        assert_eq!(CopyMode::default(), CopyMode::MarkdownSource);
    }

    #[test]
    fn normalizes_crlf_and_cr_to_lf() {
        let input = "Line 1\r\nLine 2\rLine 3\n\r\nLine 5";
        assert_eq!(
            normalize_newlines(input),
            "Line 1\nLine 2\nLine 3\n\nLine 5"
        );
    }

    #[test]
    fn plain_mode_writes_plain_only_and_strips_markdown() {
        let md = "# Sprint Notes\r\nFirst line with **bold** and *italic*.\r\nSecond line with [[Project Plan|the plan]].\r\n\r\nFourth line after blank line.";
        let payload = build_clipboard_payload(md, CopyMode::Plain);

        assert_eq!(
            payload.plain,
            "Sprint Notes\nFirst line with bold and italic.\nSecond line with the plan.\n\nFourth line after blank line."
        );
        assert_eq!(payload.html, "");
        assert_eq!(payload.line_count, 5);
    }

    #[test]
    fn rich_mode_renders_html_with_br_for_every_newline() {
        let md = "# Heading\nLine with **bold** & *italic*\n\nNext paragraph";
        let payload = build_clipboard_payload(md, CopyMode::Rich);

        assert_eq!(
            payload.plain,
            "Heading\nLine with bold & italic\n\nNext paragraph"
        );
        assert_eq!(
            payload.html,
            "<strong>Heading</strong><br>Line with <strong>bold</strong> &amp; <em>italic</em><br><br>Next paragraph"
        );
    }

    #[test]
    fn markdown_source_mode_preserves_raw_markdown_and_escapes_html() {
        let md = "# Heading\nLine with **bold** and <tag>\n[[Wiki|Alias]]";
        let payload = build_clipboard_payload(md, CopyMode::MarkdownSource);

        assert_eq!(payload.plain, md);
        assert_eq!(
            payload.html,
            "# Heading<br>Line with **bold** and &lt;tag&gt;<br>[[Wiki|Alias]]"
        );
    }

    #[test]
    fn plain_to_br_html_converts_leading_spaces_repeated_spaces_and_tabs_to_nbsp() {
        let input = "  leading two\n\tleading tab\nmid  double   triple\n \t mixed";
        let html = plain_to_br_html(input);

        assert_eq!(
            html,
            "&nbsp;&nbsp;leading two<br>&nbsp;&nbsp;&nbsp;&nbsp;leading tab<br>mid &nbsp;double &nbsp;&nbsp;triple<br>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;mixed"
        );

        let rich_payload = build_clipboard_payload("  Indented **bold**  text\n\tTabbed line", CopyMode::Rich);
        assert_eq!(
            rich_payload.html,
            "&nbsp;&nbsp;Indented <strong>bold</strong> &nbsp;text<br>&nbsp;&nbsp;&nbsp;&nbsp;Tabbed line"
        );
    }

    #[test]
    fn code_fences_are_dropped_in_plain_and_rich_so_line_count_matches_content() {
        let fenced = "```rust\nfn main() {\n    println!(\"hi\");\n}\n```";
        let plain_payload = build_clipboard_payload(fenced, CopyMode::Plain);
        assert_eq!(plain_payload.plain, "fn main() {\n    println!(\"hi\");\n}");
        assert_eq!(plain_payload.line_count, 3);

        let rich_payload = build_clipboard_payload(fenced, CopyMode::Rich);
        assert_eq!(rich_payload.plain, "fn main() {\n    println!(\"hi\");\n}");
        assert_eq!(rich_payload.line_count, 3);
        assert_eq!(
            rich_payload.html,
            "<code>fn main() {</code><br><code>&nbsp;&nbsp;&nbsp;&nbsp;println!(&quot;hi&quot;);</code><br><code>}</code>"
        );

        let tilde_fenced = "~~~\nline 1\nline 2\n~~~";
        let tilde_plain = build_clipboard_payload(tilde_fenced, CopyMode::Plain);
        assert_eq!(tilde_plain.plain, "line 1\nline 2");
        assert_eq!(tilde_plain.line_count, 2);
    }

    #[test]
    fn wiki_links_inside_inline_code_are_not_rewritten() {
        let md = "Use `[[Note|Alias]]` in code, but resolve [[Note|Alias]] outside.";
        let plain_payload = build_clipboard_payload(md, CopyMode::Plain);
        assert_eq!(
            plain_payload.plain,
            "Use [[Note|Alias]] in code, but resolve Alias outside."
        );

        let rich_payload = build_clipboard_payload(md, CopyMode::Rich);
        assert_eq!(
            rich_payload.html,
            "Use <code>[[Note|Alias]]</code> in code, but resolve Alias outside."
        );
    }

    #[test]
    fn multiple_consecutive_blank_lines_become_matching_br_tags() {
        let text = "A\n\n\nB\n";
        let payload = build_clipboard_payload(text, CopyMode::MarkdownSource);
        assert_eq!(payload.plain, "A\n\n\nB\n");
        assert_eq!(payload.html, "A<br><br><br>B<br>");
    }

    #[test]
    #[cfg_attr(target_os = "linux", ignore = "requires a desktop clipboard; run explicitly in an X11 or Wayland session")]
    fn system_clipboard_roundtrip_writes_html_and_plain_via_arboard() {
        // 1. Rich mode writes both CF_UNICODETEXT and CF_HTML (with <br> and &nbsp;)
        let sample = "# Heading\r\n  Indented **bold**  line\r\n\tTabbed line";
        let rich_payload = build_clipboard_payload(sample, CopyMode::Rich);
        write_to_system_clipboard(&rich_payload)
            .expect("Rich mode clipboard write must succeed");

        let read_back_plain = read_plain_from_system_clipboard()
            .expect("Reading plain text from clipboard must succeed");
        assert_eq!(
            read_back_plain,
            "Heading\n  Indented bold  line\n\tTabbed line"
        );

        #[cfg(target_os = "windows")]
        {
            let cf_html = read_html_from_system_clipboard()
                .expect("Reading CF_HTML from clipboard must succeed after Rich mode write");
            assert!(
                cf_html.contains("<br>"),
                "CF_HTML should contain <br> line breaks, got: {cf_html}"
            );
            assert!(
                cf_html.contains("&nbsp;&nbsp;Indented <strong>bold</strong> &nbsp;line"),
                "CF_HTML should contain &nbsp; for leading and repeated spaces, got: {cf_html}"
            );
            assert!(
                cf_html.contains("&nbsp;&nbsp;&nbsp;&nbsp;Tabbed line"),
                "CF_HTML should contain four &nbsp; for tabs, got: {cf_html}"
            );
        }

        // 2. Plain mode writes text/plain only (set_text) with no CF_HTML
        let plain_payload = build_clipboard_payload(sample, CopyMode::Plain);
        write_to_system_clipboard(&plain_payload)
            .expect("Plain mode clipboard write must succeed");

        let read_back_plain_only = read_plain_from_system_clipboard()
            .expect("Reading plain text after Plain mode write must succeed");
        assert_eq!(
            read_back_plain_only,
            "Heading\n  Indented bold  line\n\tTabbed line"
        );

        #[cfg(target_os = "windows")]
        {
            let cf_html_after_plain = read_html_from_system_clipboard();
            assert!(
                cf_html_after_plain.is_err(),
                "Plain mode must not put CF_HTML on the clipboard, but found: {cf_html_after_plain:?}"
            );
        }
    }
}
