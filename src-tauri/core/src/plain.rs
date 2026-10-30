//! Plain-text rendering of Markdown, shared by the clipboard and the search index.

use crate::links::replace_wiki_links_with_display;

/// Normalizes all Windows (`\r\n`) and legacy Mac (`\r`) line endings to LF (`\n`),
/// preserving exact line break counts (including leading, trailing, and consecutive blank lines).
pub fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

/// Strips inline Markdown formatting (`**bold**`, `*italic*`, `~~strike~~`, `` `code` ``,
/// `[label](url)`, `![alt](url)`, `[[wiki|alias]]`) from a single line.
/// Wiki links inside inline code spans (`` `[[wiki]]` ``) are not rewritten.
pub fn strip_inline_markdown(line: &str) -> String {
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
