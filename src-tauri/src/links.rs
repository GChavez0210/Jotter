use crate::types::WikiLink;

/// Splits optional leading YAML frontmatter (`---\n...\n---\n`) from a Markdown document.
/// Returns `(body, Some(frontmatter_inner))` if valid frontmatter is present at byte 0,
/// or `(content, None)` otherwise.
pub fn split_yaml_frontmatter(content: &str) -> (&str, Option<&str>) {
    let rest = if let Some(r) = content.strip_prefix("---\n") {
        r
    } else if let Some(r) = content.strip_prefix("---\r\n") {
        r
    } else {
        return (content, None);
    };

    let mut search_from = 0;
    while let Some(pos) = rest[search_from..].find("\n---") {
        let idx = search_from + pos;
        let after_marker = &rest[idx + 4..];
        if let Some(body) = after_marker.strip_prefix('\n') {
            return (body, Some(&rest[..idx]));
        } else if let Some(body) = after_marker.strip_prefix("\r\n") {
            return (body, Some(&rest[..idx]));
        } else if after_marker.is_empty() {
            return ("", Some(&rest[..idx]));
        }
        search_from = idx + 4;
    }

    (content, None)
}

/// Parses a single inner wiki-link string (inside `[[...]]`) into its components:
/// `Note`, `Note|Alias`, `Note#Heading`, or `Note#Heading|Alias`.
pub fn parse_wiki_link_inner(raw: &str, inner: &str) -> Option<WikiLink> {
    let trimmed = inner.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }

    let (target_and_heading, alias) = match trimmed.split_once('|') {
        Some((left, right)) => {
            let alias_trimmed = right.trim();
            (
                left.trim(),
                if alias_trimmed.is_empty() {
                    None
                } else {
                    Some(alias_trimmed.to_string())
                },
            )
        }
        None => (trimmed, None),
    };

    let (target, heading) = match target_and_heading.split_once('#') {
        Some((t, h)) => {
            let h_trimmed = h.trim();
            (
                t.trim().to_string(),
                if h_trimmed.is_empty() {
                    None
                } else {
                    Some(h_trimmed.to_string())
                },
            )
        }
        None => (target_and_heading.to_string(), None),
    };

    if target.is_empty() && heading.is_none() {
        return None;
    }

    Some(WikiLink {
        raw: raw.to_string(),
        target,
        heading,
        alias,
    })
}

/// Extracts all `[[wiki links]]` from Markdown content, skipping fenced code blocks and inline code.
pub fn parse_wiki_links(content: &str) -> Vec<WikiLink> {
    let (body, _) = split_yaml_frontmatter(content);
    let mut links = Vec::new();
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        let mut in_inline_code = false;

        while i < len {
            if bytes[i] == b'`' {
                in_inline_code = !in_inline_code;
                i += 1;
                continue;
            }
            if in_inline_code {
                i += 1;
                continue;
            }
            if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
                // Skip escaped `\[[`
                if i > 0 && bytes[i - 1] == b'\\' {
                    i += 2;
                    continue;
                }
                let start = i;
                let inner_start = i + 2;
                if let Some(rel_end) = line[inner_start..].find("]]") {
                    let inner_end = inner_start + rel_end;
                    let raw = &line[start..inner_end + 2];
                    let inner = &line[inner_start..inner_end];
                    if let Some(parsed) = parse_wiki_link_inner(raw, inner) {
                        links.push(parsed);
                    }
                    i = inner_end + 2;
                    continue;
                }
            }
            i += 1;
        }
    }

    links
}

/// Replaces `[[wiki links]]` in a single line with their human-readable display text,
/// while leaving `[[wiki links]]` inside inline code spans (`` `...` ``) untouched:
/// - `[[Note|Alias]]` -> `Alias`
/// - `[[Note#Heading|Alias]]` -> `Alias`
/// - `[[Note#Heading]]` -> `Note > Heading`
/// - `[[#Heading]]` -> `Heading`
/// - `[[Note]]` -> `Note`
pub fn replace_wiki_links_with_display(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Preserve inline code spans (`...` or ``...``) verbatim
        if bytes[i] == b'`' {
            let tick_start = i;
            while i < len && bytes[i] == b'`' {
                i += 1;
            }
            let tick_len = i - tick_start;
            let ticks = &line[tick_start..i];

            if let Some(rel_close) = line[i..].find(ticks) {
                let close_end = i + rel_close + tick_len;
                out.push_str(&line[tick_start..close_end]);
                i = close_end;
                continue;
            } else {
                out.push_str(&line[tick_start..]);
                break;
            }
        }

        // Check for `![[` (embed) or `[[` (wiki link)
        let is_embed =
            bytes[i] == b'!' && i + 2 < len && bytes[i + 1] == b'[' && bytes[i + 2] == b'[';
        let is_wiki = bytes[i] == b'[' && i + 1 < len && bytes[i + 1] == b'[';

        if is_embed || is_wiki {
            // Check for escaped `\[[` or `\![[`
            if i > 0 && bytes[i - 1] == b'\\' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            let bracket_open = if is_embed { i + 1 } else { i };
            let inner_start = bracket_open + 2;

            // Only match `]]` that does not cross a backtick
            let after_open = &line[inner_start..];
            if let Some(close_rel) = after_open.find("]]") {
                let inner = &after_open[..close_rel];
                if !inner.contains('`') {
                    let raw = &line[bracket_open..inner_start + close_rel + 2];
                    if let Some(link) = parse_wiki_link_inner(raw, inner) {
                        if let Some(alias) = link.alias {
                            out.push_str(&alias);
                        } else if let Some(heading) = link.heading {
                            if link.target.is_empty() {
                                out.push_str(&heading);
                            } else {
                                out.push_str(&link.target);
                                out.push_str(" > ");
                                out.push_str(&heading);
                            }
                        } else {
                            out.push_str(&link.target);
                        }
                    } else {
                        out.push_str(&line[i..inner_start + close_rel + 2]);
                    }
                    i = inner_start + close_rel + 2;
                    continue;
                }
            }
        }

        let ch = line[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

/// Extracts unique `#tags` from Markdown content (ignoring YAML frontmatter, code blocks, and headings).
pub fn extract_tags(content: &str) -> Vec<String> {
    let (body, _) = split_yaml_frontmatter(content);
    let mut tags = Vec::new();
    let mut in_fence = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut in_code = false;

        while i < chars.len() {
            if chars[i] == '`' {
                in_code = !in_code;
                i += 1;
                continue;
            }
            if in_code {
                i += 1;
                continue;
            }
            if chars[i] == '#' {
                let prev_ok = i == 0 || chars[i - 1].is_whitespace();
                if prev_ok && i + 1 < chars.len() {
                    let mut j = i + 1;
                    while j < chars.len()
                        && (chars[j].is_alphanumeric()
                            || chars[j] == '_'
                            || chars[j] == '-'
                            || chars[j] == '/')
                    {
                        j += 1;
                    }
                    if j > i + 1 {
                        let candidate: String = chars[i + 1..j].iter().collect();
                        // Obsidian requirement: tag cannot be purely numeric (e.g. #123)
                        if candidate.chars().any(|c| c.is_alphabetic())
                            && !tags.contains(&candidate)
                        {
                            tags.push(candidate);
                        }
                        i = j;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_wiki_link_forms() {
        let md = "Links: [[Simple Note]], [[Folder/Note|Custom Alias]], [[Architecture#Overview]], and [[Spec#API|API Docs]].";
        let links = parse_wiki_links(md);
        assert_eq!(links.len(), 4);

        assert_eq!(
            links[0],
            WikiLink {
                raw: "[[Simple Note]]".to_string(),
                target: "Simple Note".to_string(),
                heading: None,
                alias: None,
            }
        );
        assert_eq!(
            links[1],
            WikiLink {
                raw: "[[Folder/Note|Custom Alias]]".to_string(),
                target: "Folder/Note".to_string(),
                heading: None,
                alias: Some("Custom Alias".to_string()),
            }
        );
        assert_eq!(
            links[2],
            WikiLink {
                raw: "[[Architecture#Overview]]".to_string(),
                target: "Architecture".to_string(),
                heading: Some("Overview".to_string()),
                alias: None,
            }
        );
        assert_eq!(
            links[3],
            WikiLink {
                raw: "[[Spec#API|API Docs]]".to_string(),
                target: "Spec".to_string(),
                heading: Some("API".to_string()),
                alias: Some("API Docs".to_string()),
            }
        );
    }

    #[test]
    fn ignores_wiki_links_inside_code() {
        let md = "Inline `[[Ignored]]` and:\n```\n[[Also Ignored]]\n```\nReal [[Kept]]";
        let links = parse_wiki_links(md);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Kept");
    }

    #[test]
    fn replaces_wiki_links_with_readable_display() {
        let line = "See [[Roadmap|Q4 Plan]] and [[Design#Colors]] or [[Simple]].";
        assert_eq!(
            replace_wiki_links_with_display(line),
            "See Q4 Plan and Design > Colors or Simple."
        );
    }

    #[test]
    fn splits_yaml_frontmatter_and_extracts_tags() {
        let doc = "---\ntitle: Note\ntags: [meta]\n---\n# Heading\nBody with #work and #project/v2 but not #123 or `code #ignored`.";
        let (body, fm) = split_yaml_frontmatter(doc);
        assert_eq!(fm, Some("title: Note\ntags: [meta]"));
        assert!(body.starts_with("# Heading"));

        let tags = extract_tags(doc);
        assert_eq!(tags, vec!["work".to_string(), "project/v2".to_string()]);
    }
}
