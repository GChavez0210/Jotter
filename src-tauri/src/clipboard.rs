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

/// Converts normalized plain text (`\n` line breaks) into HTML where every `\n` becomes `<br>`.
pub fn plain_to_br_html(plain_normalized: &str) -> String {
    plain_normalized
        .split('\n')
        .map(escape_html)
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Strips inline Markdown formatting (`**bold**`, `*italic*`, `~~strike~~`, `` `code` ``,
/// `[label](url)`, `![alt](url)`, `[[wiki|alias]]`) from a single line.
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

        // Inline code span: `code`
        if chars[i] == '`' {
            let mut j = i + 1;
            while j < len && chars[j] != '`' {
                j += 1;
            }
            if j < len {
                for &c in &chars[i + 1..j] {
                    out.push(c);
                }
                i = j + 1;
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
/// every single `\n` newline 1-to-1.
pub fn strip_markdown_to_plain(markdown: &str) -> String {
    let normalized = normalize_newlines(markdown);
    let mut in_fence = false;
    let mut out_lines = Vec::new();

    for line in normalized.split('\n') {
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            in_fence = !in_fence;
            out_lines.push(String::new());
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
    if let Some(inner) = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        inner.to_string()
    } else {
        trimmed.replace('\n', "<br>")
    }
}

/// Converts Markdown to rich HTML via `comrak` while ensuring every single `\n` in the source
/// becomes `<br>` (so consecutive lines and blank lines paste into Gmail, Word, Slack, and
/// Google Chat with exact Notepad-identical line breaks).
pub fn markdown_to_br_html(markdown: &str) -> String {
    let normalized = normalize_newlines(markdown);
    let mut in_fence = false;
    let mut html_lines = Vec::new();

    for line in normalized.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            html_lines.push(String::new());
            continue;
        }
        if in_fence {
            html_lines.push(format!("<code>{}</code>", escape_html(line)));
            continue;
        }

        // Heading (`# Heading` -> `<strong>Heading</strong>`)
        let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hash_count) && trimmed[hash_count..].starts_with(' ') {
            let heading_text = trimmed[hash_count + 1..].trim_start();
            html_lines.push(format!(
                "<strong>{}</strong>",
                render_inline_line_with_comrak(heading_text)
            ));
            continue;
        }

        // Task list item
        let mut handled_task = false;
        for prefix in ["- [ ] ", "* [ ] ", "+ [ ] "] {
            if let Some(after_task) = trimmed.strip_prefix(prefix) {
                html_lines.push(format!(
                    "&#9744; {}",
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
                    "&#9745; {}",
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
                "&bull; {}",
                render_inline_line_with_comrak(after_bullet)
            ));
            continue;
        }

        // Blockquote (`> quote`)
        if let Some(after_quote) = trimmed.strip_prefix('>') {
            let quote_body = after_quote.strip_prefix(' ').unwrap_or(after_quote);
            html_lines.push(format!(
                "<em>{}</em>",
                render_inline_line_with_comrak(quote_body)
            ));
            continue;
        }

        html_lines.push(render_inline_line_with_comrak(line));
    }

    html_lines.join("<br>")
}

/// Builds the dual-format (`text/plain` + `text/html`) clipboard payload for the requested `CopyMode`.
///
/// - `Plain`: strips Markdown formatting to clean plain text with `\n` line breaks, and builds
///   HTML-escaped `text/html` where every `\n` is `<br>`.
/// - `Rich`: renders inline/block Markdown via `comrak` into `text/html` where every `\n` is `<br>`,
///   paired with the stripped plain-text alternative (`\n` line breaks).
/// - `MarkdownSource`: preserves raw Markdown syntax in `text/plain` (`\n` line breaks) and
///   HTML-escaped `text/html` (`<br>` line breaks).
pub fn build_clipboard_payload(text: &str, mode: CopyMode) -> ClipboardPayload {
    let normalized = normalize_newlines(text);

    let (plain, html) = match mode {
        CopyMode::Plain => {
            let stripped = strip_markdown_to_plain(&normalized);
            let br_html = plain_to_br_html(&stripped);
            (stripped, br_html)
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

/// Writes both `text/html` (with `<br>` line breaks) and `text/plain` (with `\n` line breaks)
/// to the system clipboard at once using `arboard::Clipboard::set_html`.
pub fn write_to_system_clipboard(payload: &ClipboardPayload) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to open system clipboard: {e}"))?;
    clipboard
        .set_html(&payload.html, Some(&payload.plain))
        .map_err(|e| format!("Failed to write HTML and plain text to clipboard: {e}"))
}

/// Reads plain text from the system clipboard via `arboard`, normalizing line endings to `\n`
/// for the `Ctrl+Shift+V` paste-as-plain-text command.
pub fn read_plain_from_system_clipboard() -> Result<String, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to open system clipboard: {e}"))?;
    let raw = clipboard
        .get_text()
        .map_err(|e| format!("Failed to read plain text from clipboard: {e}"))?;
    Ok(normalize_newlines(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_crlf_and_cr_to_lf() {
        let input = "Line 1\r\nLine 2\rLine 3\n\r\nLine 5";
        assert_eq!(
            normalize_newlines(input),
            "Line 1\nLine 2\nLine 3\n\nLine 5"
        );
    }

    #[test]
    fn plain_mode_preserves_exact_newlines_and_strips_markdown() {
        let md = "# Sprint Notes\r\nFirst line with **bold** and *italic*.\r\nSecond line with [[Project Plan|the plan]].\r\n\r\nFourth line after blank line.";
        let payload = build_clipboard_payload(md, CopyMode::Plain);

        assert_eq!(
            payload.plain,
            "Sprint Notes\nFirst line with bold and italic.\nSecond line with the plan.\n\nFourth line after blank line."
        );
        assert_eq!(
            payload.html,
            "Sprint Notes<br>First line with bold and italic.<br>Second line with the plan.<br><br>Fourth line after blank line."
        );
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
    fn multiple_consecutive_blank_lines_become_matching_br_tags() {
        let text = "A\n\n\nB\n";
        let payload = build_clipboard_payload(text, CopyMode::Plain);
        assert_eq!(payload.plain, "A\n\n\nB\n");
        assert_eq!(payload.html, "A<br><br><br>B<br>");
    }

    #[test]
    fn system_clipboard_roundtrip_writes_html_and_plain_via_arboard() {
        let sample = "Line 1 with **bold**\r\nLine 2 right below\r\n\r\nLine 4";
        let payload = build_clipboard_payload(sample, CopyMode::Plain);
        if write_to_system_clipboard(&payload).is_ok() {
            let read_back = read_plain_from_system_clipboard().expect("read back plain text");
            assert_eq!(
                read_back,
                "Line 1 with bold\nLine 2 right below\n\nLine 4"
            );
        }
    }
}
