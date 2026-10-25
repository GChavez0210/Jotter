//! Finds the parts of a Markdown note that hold prose, so link scanners skip code.

use std::ops::Range;

/// A line outside fenced and indented code blocks.
pub(crate) struct ProseLine<'a> {
    /// Zero-based line number within the scanned text.
    pub index: usize,
    /// Byte offset of the line start within the scanned text.
    pub offset: usize,
    /// The line without its line break.
    pub text: &'a str,
}

struct Fence {
    marker: u8,
    len: usize,
}

/// The fence a line opens or closes: up to three spaces, then three or more backticks or tildes.
fn fence_run(line: &str) -> Option<(u8, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = *rest.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = rest.bytes().take_while(|&b| b == marker).count();
    (len >= 3).then(|| (marker, len, &rest[len..]))
}

fn is_indented(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("    ")
}

fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix(['-', '*', '+']) {
        return rest.is_empty() || rest.starts_with([' ', '\t']);
    }
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && t[digits..].starts_with(['.', ')'])
        && t[digits + 1..].chars().next().is_none_or(char::is_whitespace)
}

/// Lines outside code blocks. A fence closes only with the same character, at least as long,
/// and nothing after it. An indented line is code only after a blank line and outside a list,
/// since list items indent their own paragraphs.
pub(crate) fn prose_lines(text: &str) -> Vec<ProseLine<'_>> {
    let mut out = Vec::new();
    let mut fence: Option<Fence> = None;
    let mut offset = 0;
    let mut prev_blank = true;
    let mut in_indented_code = false;
    let mut in_list = false;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches(['\n', '\r']);
        let line_offset = offset;
        offset += raw.len();
        let blank = line.trim().is_empty();

        if let Some(open) = &fence {
            if let Some((marker, len, rest)) = fence_run(line) {
                if marker == open.marker && len >= open.len && rest.trim().is_empty() {
                    fence = None;
                }
            }
            prev_blank = false;
            continue;
        }
        if let Some((marker, len, rest)) = fence_run(line) {
            // A backtick fence's info string cannot hold a backtick.
            if marker == b'~' || !rest.contains('`') {
                fence = Some(Fence { marker, len });
                in_indented_code = false;
                prev_blank = false;
                continue;
            }
        }
        if blank {
            prev_blank = true;
            continue;
        }
        if is_indented(line) && (in_indented_code || (prev_blank && !in_list)) {
            in_indented_code = true;
            prev_blank = false;
            continue;
        }
        in_indented_code = false;
        if !is_indented(line) {
            in_list = is_list_item(line);
        }
        prev_blank = false;
        out.push(ProseLine { index, offset: line_offset, text: line });
    }
    out
}

/// Byte ranges of inline code spans in one line. A run of backticks closes only with a run of the
/// same length; an unmatched run is plain text.
pub(crate) fn inline_code_spans(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' || (i > 0 && bytes[i - 1] == b'\\') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let run = i - start;
        let mut j = i;
        let mut closed = None;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let close_start = j;
                while j < bytes.len() && bytes[j] == b'`' {
                    j += 1;
                }
                if j - close_start == run {
                    closed = Some(j);
                    break;
                }
            } else {
                j += 1;
            }
        }
        match closed {
            Some(end) => {
                spans.push(start..end);
                i = end;
            }
            None => i = start + run,
        }
    }
    spans
}

pub(crate) fn in_spans(spans: &[Range<usize>], pos: usize) -> bool {
    spans.iter().any(|s| s.contains(&pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(text: &str) -> Vec<&str> {
        prose_lines(text).into_iter().map(|l| l.text).collect()
    }

    #[test]
    fn fences_close_only_with_a_matching_run() {
        let text = "a\n````\n```\n~~~\nb\n````\nc\n~~~\n```\nd\n~~~\ne";
        assert_eq!(kept(text), vec!["a", "c", "e"]);
    }

    #[test]
    fn indented_code_needs_a_blank_line_and_no_list() {
        let text = "para\n    not code\n\n    code\n    more\n\n- item\n\n    list text\nend";
        assert_eq!(kept(text), vec!["para", "    not code", "- item", "    list text", "end"]);
    }

    #[test]
    fn inline_code_matches_backtick_runs() {
        let line = "a ``x ` y`` b `z` c ``` d";
        let spans = inline_code_spans(line);
        assert_eq!(spans.len(), 2);
        assert_eq!(&line[spans[0].clone()], "``x ` y``");
        assert_eq!(&line[spans[1].clone()], "`z`");
    }
}
