//! Protects Markdown that a model must not change during rewrite and translate.
//!
//! `mask_markdown` swaps code, math, links, tags, and similar spans for numbered
//! placeholders such as `⟦J0⟧`. `unmask_markdown` puts them back and fails when the
//! model dropped, duplicated, or invented a placeholder, so a bad reply never
//! reaches the note.

const OPEN: char = '⟦';
const CLOSE: char = '⟧';

use comrak::{nodes::NodeValue, parse_document, Arena, Options};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked {
    pub text: String,
    pub tokens: Vec<String>,
}

pub fn placeholder(index: usize) -> String {
    format!("{OPEN}J{index}{CLOSE}")
}

struct Masker {
    out: String,
    tokens: Vec<String>,
}

impl Masker {
    fn protect(&mut self, span: &str) {
        self.out.push_str(&placeholder(self.tokens.len()));
        self.tokens.push(span.to_string());
    }

    /// Protects a run of whole lines, leaving the final line break outside the placeholder
    /// so the model still sees the line structure.
    fn protect_lines(&mut self, lines: &[&str]) {
        let joined: String = lines.concat();
        let body = joined.trim_end_matches(['\n', '\r']);
        let ending = &joined[body.len()..];
        self.protect(body);
        self.out.push_str(ending);
    }
}

pub fn mask_markdown(input: &str) -> Result<Masked, String> {
    if input.contains(OPEN) || input.contains(CLOSE) {
        return Err(format!(
            "The text contains {OPEN} or {CLOSE}, which Jotter reserves to protect Markdown. Remove them and try again."
        ));
    }
    let mut masker = Masker {
        out: String::with_capacity(input.len()),
        tokens: Vec::new(),
    };
    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let mut i = 0;
    if lines.first().is_some_and(|l| l.trim_end() == "---") {
        if let Some(close) = (1..lines.len()).find(|&j| matches!(lines[j].trim_end(), "---" | "...")) {
            masker.protect_lines(&lines[..=close]);
            i = close + 1;
        }
    }
    // Use the Markdown parser's block context for indented code and fences inside
    // lists or blockquotes. Inline scanning alone cannot recognize these safely.
    let arena = Arena::new();
    let body = lines[i..].concat();
    let root = parse_document(&arena, &body, &Options::default());
    let code_blocks: std::collections::HashMap<usize, usize> = root.descendants()
        .filter_map(|node| {
            let data = node.data.borrow();
            matches!(data.value, NodeValue::CodeBlock(_)).then(|| (
                i + data.sourcepos.start.line.saturating_sub(1),
                (i + data.sourcepos.end.line.saturating_sub(1)).min(lines.len().saturating_sub(1)),
            ))
        })
        .collect();
    while i < lines.len() {
        let line = lines[i];
        if let Some(&close) = code_blocks.get(&i) {
            masker.protect_lines(&lines[i..=close]);
            i = close + 1;
            continue;
        }
        if is_reference_definition(line) {
            // Definitions can put their destination and quoted title on following
            // lines. Preserve the complete paragraph, including relative paths.
            let close = (i + 1..lines.len())
                .find(|&j| lines[j].trim().is_empty())
                .unwrap_or(lines.len());
            masker.protect_lines(&lines[i..close]);
            i = close;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("$$") && !(trimmed.len() > 4 && trimmed.ends_with("$$")) {
            let close = (i + 1..lines.len())
                .find(|&j| lines[j].contains("$$"))
                .unwrap_or(lines.len() - 1);
            masker.protect_lines(&lines[i..=close]);
            i = close + 1;
            continue;
        }
        mask_line(&mut masker, line);
        i += 1;
    }

    Ok(Masked {
        text: masker.out,
        tokens: masker.tokens,
    })
}

fn is_reference_definition(line: &str) -> bool {
    let mut content = line.trim_start();
    loop {
        if let Some(rest) = content.strip_prefix('>') {
            content = rest.trim_start();
        } else if let Some(rest) = ["- ", "+ ", "* "].iter().find_map(|p| content.strip_prefix(p)) {
            content = rest.trim_start();
        } else {
            let digits = content.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && (content[digits..].starts_with(". ") || content[digits..].starts_with(") ")) {
                content = content[digits + 2..].trim_start();
            } else {
                break;
            }
        }
    }
    content.starts_with('[') && content.find("]:").is_some_and(|end| end > 1)
}

/// Masks inline spans on one line. Keeps the line ending outside any placeholder.
fn mask_line(masker: &mut Masker, line: &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    let ending = &line[body.len()..];
    let (content, block_id) = split_block_id(body);
    mask_inline(masker, content);
    if let Some(id) = block_id {
        masker.out.push(' ');
        masker.protect(id);
    }
    masker.out.push_str(ending);
}

/// Splits an Obsidian block ID such as ` ^abc-123` off the end of a line.
fn split_block_id(body: &str) -> (&str, Option<&str>) {
    if let Some(pos) = body.rfind(" ^") {
        let id = &body[pos + 1..];
        if id.len() > 1 && id[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return (&body[..pos], Some(id));
        }
    }
    (body, None)
}

fn mask_inline(masker: &mut Masker, text: &str) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut plain_start = 0;
    while i < bytes.len() {
        let span_end = match bytes[i] {
            b'\\' => {
                // Keep an escape and the character it escapes together as plain text.
                i += 1;
                if i < bytes.len() {
                    i += utf8_len(bytes[i]);
                }
                continue;
            }
            b'`' => code_span_end(text, i),
            b'$' => math_span_end(text, i),
            b'!' if text[i + 1..].starts_with("[[") => wiki_end(text, i + 1),
            b'[' if text[i..].starts_with("[[") => wiki_end(text, i),
            b'[' if text[i..].starts_with("[^") => text[i..].find(']').map(|p| i + p + 1),
            b'[' => {
                if let Some((label_end, dest_end)) = link_parts(text, i) {
                    masker.out.push_str(&text[plain_start..i]);
                    masker.out.push('[');
                    mask_inline(masker, &text[i + 1..label_end]);
                    masker.out.push(']');
                    masker.protect(&text[label_end + 1..dest_end]);
                    i = dest_end;
                    plain_start = i;
                    continue;
                }
                None
            }
            b'<' => html_or_autolink_end(text, i),
            b'h' if starts_url(text, i) => Some(url_end(text, i)),
            b'#' if starts_tag(text, i) => Some(tag_end(text, i)),
            _ => None,
        };
        match span_end {
            Some(end) => {
                masker.out.push_str(&text[plain_start..i]);
                masker.protect(&text[i..end]);
                i = end;
                plain_start = i;
            }
            None => i += utf8_len(bytes[i]),
        }
    }
    masker.out.push_str(&text[plain_start..]);
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn run_len(text: &str, at: usize, byte: u8) -> usize {
    text.as_bytes()[at..].iter().take_while(|b| **b == byte).count()
}

/// End of a code span that opens with a run of backticks and closes with a run of the same length.
fn code_span_end(text: &str, start: usize) -> Option<usize> {
    let open = run_len(text, start, b'`');
    let mut i = start + open;
    while let Some(p) = text[i..].find('`') {
        let at = i + p;
        let len = run_len(text, at, b'`');
        if len == open {
            return Some(at + len);
        }
        i = at + len;
    }
    None
}

/// End of `$…$` or `$$…$$` on one line. A `$` followed by a space or digit, or a closing `$`
/// preceded by a space, is not math, so prices such as `$5 and $10` stay plain text.
fn math_span_end(text: &str, start: usize) -> Option<usize> {
    if text[start..].starts_with("$$") {
        return text[start + 2..].find("$$").map(|p| start + 2 + p + 2);
    }
    let after = text[start + 1..].chars().next()?;
    if after.is_whitespace() || after == '$' || after.is_ascii_digit() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while let Some(p) = text[i..].find('$') {
        let at = i + p;
        let before = bytes[at - 1];
        let next_is_digit = bytes.get(at + 1).is_some_and(|b| b.is_ascii_digit());
        if before != b' ' && before != b'\\' && !next_is_digit {
            return Some(at + 1);
        }
        i = at + 1;
    }
    None
}

fn wiki_end(text: &str, start: usize) -> Option<usize> {
    text[start + 2..].find("]]").map(|p| start + 2 + p + 2)
}

/// For `[label](dest)` or `[label][ref]`, returns the index of the `]` that closes the label
/// and the end of the destination.
fn link_parts(text: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    let label_end = loop {
        match bytes.get(i)? {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            _ => {}
        }
        i += 1;
    };
    let (open, close) = match bytes.get(label_end + 1)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut j = label_end + 1;
    while let Some(b) = bytes.get(j) {
        if *b == b'\\' {
            j += 2;
            continue;
        }
        if *b == open {
            depth += 1;
        } else if *b == close {
            depth -= 1;
            if depth == 0 {
                return Some((label_end, j + 1));
            }
        }
        j += 1;
    }
    None
}

fn html_or_autolink_end(text: &str, start: usize) -> Option<usize> {
    let next = text[start + 1..].chars().next()?;
    if !(next.is_ascii_alphabetic() || next == '/' || next == '!') {
        return None;
    }
    text[start..].find('>').map(|p| start + p + 1)
}

fn starts_url(text: &str, start: usize) -> bool {
    let rest = &text[start..];
    (rest.starts_with("https://") || rest.starts_with("http://"))
        && text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || c == '(')
}

fn url_end(text: &str, start: usize) -> usize {
    let rest = &text[start..];
    let raw = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let trimmed = rest[..raw].trim_end_matches(['.', ',', ';', ':', '!', '?', ')']);
    start + trimmed.len()
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/')
}

fn starts_tag(text: &str, start: usize) -> bool {
    let prev_ok = text[..start].chars().next_back().is_none_or(char::is_whitespace);
    prev_ok && text[start + 1..].chars().next().is_some_and(is_tag_char)
}

fn tag_end(text: &str, start: usize) -> usize {
    let rest = &text[start + 1..];
    start + 1 + rest.find(|c: char| !is_tag_char(c)).unwrap_or(rest.len())
}

/// Restores protected spans. Fails unless every placeholder appears exactly once and no
/// unknown placeholder appears.
pub fn unmask_markdown(output: &str, tokens: &[String]) -> Result<String, String> {
    let mut problems = 0usize;
    for index in 0..tokens.len() {
        if output.matches(&placeholder(index)).count() != 1 {
            problems += 1;
        }
    }
    let total = output.matches(OPEN).count();
    if problems > 0 || total != tokens.len() {
        return Err(
            "The model changed protected Markdown such as code, links, or math. Jotter left the note unchanged. Run the command again or select less text."
                .to_string(),
        );
    }
    let mut result = output.to_string();
    // Replace from the highest index down so `⟦J1⟧` never matches inside `⟦J10⟧`.
    for (index, token) in tokens.iter().enumerate().rev() {
        result = result.replacen(&placeholder(index), token, 1);
    }
    Ok(result)
}

/// Removes a code fence that a model wrapped around its whole reply, unless the input itself
/// was a single fenced block.
pub fn strip_wrapping_fence(output: &str, input: &str) -> String {
    let trimmed = output.trim();
    if input.trim_start().starts_with("```") || !trimmed.starts_with("```") || !trimmed.ends_with("```") {
        return trimmed.to_string();
    }
    let Some(first_break) = trimmed.find('\n') else {
        return trimmed.to_string();
    };
    let inner = &trimmed[first_break + 1..trimmed.len() - 3];
    inner.trim().to_string()
}

/// Gives a rewritten passage the same leading and trailing whitespace as the original, so a
/// replacement does not merge or split paragraphs.
pub fn keep_surrounding_whitespace(original: &str, replacement: &str) -> String {
    let lead = &original[..original.len() - original.trim_start().len()];
    let trail = &original[original.trim_end().len()..];
    format!("{lead}{}{trail}", replacement.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(input: &str) -> Masked {
        let masked = mask_markdown(input).unwrap();
        assert_eq!(unmask_markdown(&masked.text, &masked.tokens).unwrap(), input);
        masked
    }

    #[test]
    fn protects_code_links_math_and_tags_but_not_prose() {
        let input = "Read `cfg.toml` and [the guide](https://x.io/a_(b)) with [[Plan|our plan]].\n\
                     Cost is $5 and $10, area is $a^2$. Tagged #work/q3 and see https://ex.com/p.\n\
                     ![[img.png]] <br> [^1] [ref][r1] ^block-1\n";
        let masked = roundtrip(input);
        assert_eq!(
            masked.text,
            "Read ⟦J0⟧ and [the guide]⟦J1⟧ with ⟦J2⟧.\n\
             Cost is $5 and $10, area is ⟦J3⟧. Tagged ⟦J4⟧ and see ⟦J5⟧.\n\
             ⟦J6⟧ ⟦J7⟧ ⟦J8⟧ [ref]⟦J9⟧ ⟦J10⟧\n"
        );
        assert_eq!(masked.tokens[1], "(https://x.io/a_(b))");
        assert_eq!(masked.tokens[5], "https://ex.com/p");
    }

    #[test]
    fn protects_fenced_blocks_math_blocks_and_frontmatter_as_whole_lines() {
        let input = "---\ntags: [a]\n---\nIntro\n```rust\nlet x = 1; // [[not a link]]\n```\n$$\nx^2\n$$\n~~~\nraw\n";
        let masked = roundtrip(input);
        assert_eq!(masked.text, "⟦J0⟧\nIntro\n⟦J1⟧\n⟦J2⟧\n⟦J3⟧\n");
        assert_eq!(masked.tokens[3], "~~~\nraw");
    }

    #[test]
    fn headings_escapes_and_link_labels_stay_translatable() {
        let masked = roundtrip("# Title\n\\#not-tag and [**bold** label](x.md)\r\n");
        assert_eq!(masked.text, "# Title\n\\#not-tag and [**bold** label]⟦J0⟧\r\n");
    }

    #[test]
    fn unmask_rejects_dropped_duplicated_or_invented_placeholders() {
        let tokens = vec!["`a`".to_string(), "`b`".to_string()];
        assert_eq!(unmask_markdown("x ⟦J1⟧ ⟦J0⟧", &tokens).unwrap(), "x `b` `a`");
        assert!(unmask_markdown("x ⟦J0⟧", &tokens).is_err());
        assert!(unmask_markdown("⟦J0⟧ ⟦J0⟧ ⟦J1⟧", &tokens).is_err());
        assert!(unmask_markdown("⟦J0⟧ ⟦J1⟧ ⟦J2⟧", &tokens).is_err());
    }

    #[test]
    fn unmask_keeps_double_digit_placeholders_apart() {
        let tokens: Vec<String> = (0..12).map(|i| format!("<{i}>")).collect();
        let text: String = (0..12).map(placeholder).collect::<Vec<_>>().join(" ");
        assert_eq!(
            unmask_markdown(&text, &tokens).unwrap(),
            (0..12).map(|i| format!("<{i}>")).collect::<Vec<_>>().join(" ")
        );
    }

    #[test]
    fn rejects_input_that_already_uses_placeholder_brackets() {
        assert!(mask_markdown("see ⟦J0⟧").is_err());
    }

    #[test]
    fn protects_indented_and_container_code_blocks() {
        for block in [
            "    dangerous_command --flag\n    next_command\n",
            "\tdangerous_command --flag\n",
            "> ```sh\n> dangerous_command --flag\n> ```\n",
            "- ```sh\n  dangerous_command --flag\n  ```\n",
            "> - ```sh\n>   dangerous_command --flag\n>   ```\n",
        ] {
            let input = format!("Intro\n\n{block}\nEnd\n");
            let masked = roundtrip(&input);
            assert!(!masked.text.contains("dangerous_command"), "{masked:?}");
            assert!(masked.text.contains("Intro") && masked.text.contains("End"));
            assert!(unmask_markdown("Intro\n\nEnd\n", &masked.tokens).is_err());
        }
    }

    #[test]
    fn protects_relative_reference_destinations_and_multiline_titles() {
        for definition in [
            "[manual]: /original.md\n",
            "[manual]:\n  ../original.md\n  \"The original title\"\n",
            "> [manual]: /original.md\n>   \"The original title\"\n",
        ] {
            let input = format!("Read [guide][manual].\n\n{definition}\nEnd\n");
            let masked = roundtrip(&input);
            assert!(!masked.text.contains("original"), "{masked:?}");
            assert!(masked.text.contains("[guide]") && masked.text.contains("End"));
            let dropped = masked.text.replace(&placeholder(masked.tokens.len() - 1), "");
            assert!(unmask_markdown(&dropped, &masked.tokens).is_err());
        }
    }

    #[test]
    fn strips_a_fence_the_model_added_and_keeps_whitespace() {
        assert_eq!(strip_wrapping_fence("```markdown\nHola\n```", "Hello"), "Hola");
        assert_eq!(strip_wrapping_fence("```js\nx\n```", "```js\ny\n```"), "```js\nx\n```");
        assert_eq!(keep_surrounding_whitespace("\n  Hello\n\n", " Hola "), "\n  Hola\n\n");
    }
}
