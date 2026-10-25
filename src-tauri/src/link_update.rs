//! Rewrites the links that point at a note when the note is renamed or moved.
//!
//! The plan is built before the rename, while the old name still resolves, and lists text edits
//! per note. Offsets count UTF-16 code units with each line break as one unit, which is how the
//! editor numbers its text, so the frontend can apply the edits to an open note as they are.

use crate::fs_atomic::atomic_write_str;
use crate::health::{collect_attachments, destination_length, join_normalized, local_destination};
use crate::index::{scan_dir_files_from_disk, WikiLinkResolver};
use crate::links::{parse_wiki_link_inner, split_yaml_frontmatter};
use crate::md_scan::{in_spans, inline_code_spans, prose_lines};
use crate::vault::{ensure_md_extension, resolve_safe_vault_path};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub from: usize,
    pub to: usize,
    /// The text expected at `from..to`. An edit is skipped when the note no longer has it.
    pub expected: String,
    pub insert: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteEdits {
    /// The note's path after the rename.
    pub path: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkUpdatePlan {
    pub notes: Vec<NoteEdits>,
    pub link_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedEdits {
    pub applied: usize,
    pub skipped: usize,
    /// Edits that put the original text back, numbered in the changed note.
    pub reverse: Vec<TextEdit>,
}

enum SpanKind {
    Wiki,
    Markdown { wrapped: bool },
}

/// The part of a link that names its target, as byte offsets into the note.
struct Span {
    from: usize,
    to: usize,
    text: String,
    kind: SpanKind,
}

/// Finds link targets outside properties and code, with their positions.
fn scan_links(content: &str) -> Vec<Span> {
    let (body, _) = split_yaml_frontmatter(content);
    // The body is always the end of the note, whether or not properties come first.
    let body_start = content.len() - body.len();
    let mut spans = Vec::new();
    for line in prose_lines(body) {
        scan_line(line.text, body_start + line.offset, &mut spans);
    }
    spans
}

fn scan_line(line: &str, offset: usize, out: &mut Vec<Span>) {
    let bytes = line.as_bytes();
    let code = inline_code_spans(line);
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            _ if in_spans(&code, i) => i += 1,
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                if i > 0 && bytes[i - 1] == b'\\' {
                    i += 2;
                    continue;
                }
                let inner_start = i + 2;
                let Some(rel_end) = line[inner_start..].find("]]") else {
                    i += 1;
                    continue;
                };
                let inner_end = inner_start + rel_end;
                let inner = &line[inner_start..inner_end];
                if let Some(parsed) = parse_wiki_link_inner(&line[i..inner_end + 2], inner) {
                    if !parsed.target.is_empty() {
                        let left = inner.split('|').next().unwrap_or(inner);
                        let segment = left.split('#').next().unwrap_or(left);
                        let lead = segment.len() - segment.trim_start().len();
                        let from = offset + inner_start + lead;
                        out.push(Span {
                            from,
                            to: from + parsed.target.len(),
                            text: parsed.target,
                            kind: SpanKind::Wiki,
                        });
                    }
                }
                i = inner_end + 2;
            }
            b']' if bytes.get(i + 1) == Some(&b'(') => {
                let start = i + 2;
                let Some(len) = destination_length(&line[start..]) else {
                    i += 1;
                    continue;
                };
                let raw = &line[start..start + len];
                if let Some((from, to, wrapped)) = path_span(raw) {
                    out.push(Span {
                        from: offset + start + from,
                        to: offset + start + to,
                        text: raw[from..to].to_string(),
                        kind: SpanKind::Markdown { wrapped },
                    });
                }
                i = start + len;
            }
            _ => i += 1,
        }
    }
}

/// The path inside a link destination, leaving out `<>`, a `#section`, a `?query`, and a title.
fn path_span(raw: &str) -> Option<(usize, usize, bool)> {
    local_destination(raw)?;
    let wrapped = raw.starts_with('<');
    let start = usize::from(wrapped);
    let rest = &raw[start..];
    let stop = if wrapped {
        rest.find('>').unwrap_or(rest.len())
    } else {
        rest.find(char::is_whitespace).unwrap_or(rest.len())
    };
    let token = &rest[..stop];
    let end = token.find(['#', '?']).unwrap_or(token.len());
    (end > 0).then_some((start, start + end, wrapped))
}

fn strip_md_ext(path: &str) -> &str {
    match path.len().checked_sub(3) {
        Some(cut) if path.is_char_boundary(cut) && path[cut..].eq_ignore_ascii_case(".md") => &path[..cut],
        _ => path,
    }
}

fn has_md_ext(path: &str) -> bool {
    strip_md_ext(path).len() != path.len()
}

/// The text to write in place of a wiki link target, or `None` when the link needs no change.
fn new_wiki_target(
    original: &str,
    new_rel: &str,
    source_after: &str,
    after: &WikiLinkResolver,
) -> Option<String> {
    let resolves_to_new = |text: &str| after.resolve(source_after, text).as_deref() == Some(new_rel);
    // A move that leaves the name unambiguous keeps `[[Name]]` working as written.
    if resolves_to_new(original) {
        return None;
    }
    let with_ext = |text: &str| {
        if has_md_ext(original) { format!("{text}.md") } else { text.to_string() }
    };
    let path = strip_md_ext(new_rel);
    let name = path.rsplit('/').next().unwrap_or(path);
    let mut candidates = Vec::new();
    // A link written as a bare name stays one while the name still finds the note.
    if !original.contains('/') && !original.contains('\\') {
        candidates.push(with_ext(name));
    }
    candidates.push(with_ext(path));
    for candidate in candidates {
        if resolves_to_new(&candidate) {
            return (candidate != original).then_some(candidate);
        }
    }
    Some(new_rel.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkdownBase {
    Source,
    Root,
}

/// How a Markdown destination reaches `files`: from the note's folder, or from the vault root.
fn resolve_markdown(
    source: &str,
    dest: &str,
    files: &HashMap<String, String>,
) -> Option<(String, MarkdownBase)> {
    let source_dir = source.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut candidates = Vec::new();
    if let Some(rooted) = dest.strip_prefix('/') {
        candidates.push((join_normalized("", rooted), MarkdownBase::Root));
    } else {
        candidates.push((join_normalized(source_dir, dest), MarkdownBase::Source));
        candidates.push((join_normalized("", dest), MarkdownBase::Root));
    }
    for (candidate, base) in candidates {
        let Some(candidate) = candidate else { continue };
        for key in [candidate.clone(), format!("{candidate}.md")] {
            if let Some(found) = files.get(&key.to_lowercase()) {
                return Some((found.clone(), base));
            }
        }
    }
    None
}

fn relative_path(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let target: Vec<&str> = to.split('/').collect();
    let common = from
        .iter()
        .zip(&target[..target.len().saturating_sub(1)])
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(&target[common..]);
    parts.join("/")
}

/// The destination text that reaches `target` from the note at `source_after`, written the way
/// the original was: rooted or relative, with or without `.md`.
fn new_markdown_target(
    span_text: &str,
    wrapped: bool,
    decoded: &str,
    base: MarkdownBase,
    target: &str,
    source_after: &str,
) -> Option<String> {
    let path = if has_md_ext(decoded) { target } else { strip_md_ext(target) };
    let text = match base {
        MarkdownBase::Root if decoded.starts_with('/') => format!("/{path}"),
        MarkdownBase::Root => path.to_string(),
        MarkdownBase::Source => {
            let dir = source_after.rsplit_once('/').map_or("", |(dir, _)| dir);
            relative_path(dir, path)
        }
    };
    let text = if wrapped {
        text
    } else {
        text.replace(' ', "%20").replace('(', "%28").replace(')', "%29")
    };
    (text != span_text).then_some(text)
}

struct ByteEdit {
    from: usize,
    to: usize,
    expected: String,
    insert: String,
}

/// Plans the edits that keep every link to `old_rel` working once the note is at `new_rel`.
/// `notes` holds each note's path and text from before the rename, and `attachments` every other
/// file. When the note changes folder, its own relative links are rewritten from the new folder.
pub fn plan_from_notes(
    notes: &[(String, String)],
    attachments: &[String],
    old_rel: &str,
    new_rel: &str,
) -> LinkUpdatePlan {
    if !notes.iter().any(|(path, _)| path == old_rel) {
        return LinkUpdatePlan::default();
    }
    let before_paths: Vec<String> = notes.iter().map(|(path, _)| path.clone()).collect();
    let after_paths: Vec<String> = before_paths
        .iter()
        .map(|path| if path == old_rel { new_rel.to_string() } else { path.clone() })
        .collect();
    let before = WikiLinkResolver::new(&before_paths);
    let after = WikiLinkResolver::new(&after_paths);
    let lower = |paths: &[String]| -> HashMap<String, String> {
        paths.iter().map(|p| (p.to_lowercase(), p.clone())).collect()
    };
    let mut files_before = lower(&before_paths);
    for path in attachments {
        files_before.entry(path.to_lowercase()).or_insert_with(|| path.clone());
    }
    let parent = |p: &str| p.rsplit_once('/').map_or(String::new(), |(dir, _)| dir.to_lowercase());
    let changes_folder = parent(old_rel) != parent(new_rel);

    let mut plan = LinkUpdatePlan::default();
    for (path, content) in notes {
        let source_after = if path == old_rel { new_rel } else { path.as_str() };
        let mut edits: Vec<ByteEdit> = Vec::new();
        for span in scan_links(content) {
            let insert = match span.kind {
                SpanKind::Wiki => {
                    if before.resolve(path, &span.text).as_deref() != Some(old_rel) {
                        continue;
                    }
                    new_wiki_target(&span.text, new_rel, source_after, &after)
                }
                SpanKind::Markdown { wrapped } => {
                    let Some(decoded) = local_destination(&span.text) else { continue };
                    match resolve_markdown(path, &decoded, &files_before) {
                        Some((found, base)) if found == old_rel => new_markdown_target(
                            &span.text, wrapped, &decoded, base, new_rel, source_after,
                        ),
                        // The moved note's relative links would now start from the wrong folder.
                        Some((found, MarkdownBase::Source)) if path == old_rel && changes_folder => {
                            new_markdown_target(
                                &span.text, wrapped, &decoded, MarkdownBase::Source, &found, source_after,
                            )
                        }
                        _ => continue,
                    }
                }
            };
            if let Some(insert) = insert {
                edits.push(ByteEdit { from: span.from, to: span.to, expected: span.text, insert });
            }
        }
        if edits.is_empty() {
            continue;
        }
        edits.sort_by_key(|e| e.from);
        let offsets: Vec<usize> = edits.iter().flat_map(|e| [e.from, e.to]).collect();
        let units = units_at(content, &offsets);
        let edits: Vec<TextEdit> = edits
            .into_iter()
            .enumerate()
            .map(|(i, e)| TextEdit {
                from: units[i * 2],
                to: units[i * 2 + 1],
                expected: e.expected,
                insert: e.insert,
            })
            .collect();
        plan.link_count += edits.len();
        plan.notes.push(NoteEdits { path: source_after.to_string(), edits });
    }
    plan.notes.sort_by(|a, b| a.path.cmp(&b.path));
    plan
}

fn unit_len(c: char, after_cr: bool) -> usize {
    if c == '\n' && after_cr { 0 } else { c.len_utf16() }
}

/// Converts ascending byte offsets into editor units.
fn units_at(content: &str, sorted_bytes: &[usize]) -> Vec<usize> {
    let mut out = Vec::with_capacity(sorted_bytes.len());
    let mut units = 0;
    let mut after_cr = false;
    let mut next = 0;
    for (idx, c) in content.char_indices() {
        while next < sorted_bytes.len() && sorted_bytes[next] <= idx {
            out.push(units);
            next += 1;
        }
        units += unit_len(c, after_cr);
        after_cr = c == '\r';
    }
    while next < sorted_bytes.len() {
        out.push(units);
        next += 1;
    }
    out
}

/// Converts ascending editor units into byte offsets. A unit inside a character has no offset.
fn bytes_at(content: &str, sorted_units: &[usize]) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(sorted_units.len());
    let mut units = 0;
    let mut after_cr = false;
    let mut next = 0;
    for (idx, c) in content.char_indices() {
        while next < sorted_units.len() && sorted_units[next] <= units {
            out.push((sorted_units[next] == units).then_some(idx));
            next += 1;
        }
        units += unit_len(c, after_cr);
        after_cr = c == '\r';
    }
    while next < sorted_units.len() {
        out.push((sorted_units[next] == units).then_some(content.len()));
        next += 1;
    }
    out
}

fn unit_count(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Applies `edits` to `content`, skipping any whose expected text is not there.
pub fn apply_edits_to_string(content: &str, edits: &[TextEdit]) -> (String, AppliedEdits) {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.from, e.to));
    let mut offsets: Vec<usize> = sorted.iter().flat_map(|e| [e.from, e.to]).collect();
    offsets.sort_unstable();
    offsets.dedup();
    let resolved: HashMap<usize, Option<usize>> =
        offsets.iter().copied().zip(bytes_at(content, &offsets)).collect();

    let mut out = String::with_capacity(content.len());
    let mut result = AppliedEdits::default();
    let mut cursor = 0;
    let mut delta: isize = 0;
    for edit in sorted {
        let range = match (resolved[&edit.from], resolved[&edit.to]) {
            (Some(from), Some(to)) if from >= cursor && from <= to => Some((from, to)),
            _ => None,
        };
        let Some((from, to)) = range.filter(|&(from, to)| content[from..to] == edit.expected) else {
            result.skipped += 1;
            continue;
        };
        out.push_str(&content[cursor..from]);
        out.push_str(&edit.insert);
        cursor = to;
        let new_from = (edit.from as isize + delta) as usize;
        result.reverse.push(TextEdit {
            from: new_from,
            to: new_from + unit_count(&edit.insert),
            expected: edit.insert.clone(),
            insert: edit.expected.clone(),
        });
        delta += unit_count(&edit.insert) as isize - (edit.to - edit.from) as isize;
        result.applied += 1;
    }
    out.push_str(&content[cursor..]);
    (out, result)
}

/// Builds the plan for renaming or moving the note `old_rel` to `new_rel`. Anything that is not
/// a note has no links to update.
pub fn plan_link_updates(vault_root: &Path, old_rel: &str, new_rel: &str) -> Result<LinkUpdatePlan, String> {
    let (old_abs, old_clean) = resolve_safe_vault_path(vault_root, old_rel)?;
    if !old_abs.is_file() || !has_md_ext(&old_clean) {
        return Ok(LinkUpdatePlan::default());
    }
    let (_, new_clean) = resolve_safe_vault_path(vault_root, &ensure_md_extension(new_rel))?;
    let notes: Vec<(String, String)> = scan_dir_files_from_disk(vault_root, vault_root)?
        .into_iter()
        .map(|note| (note.path, note.content))
        .collect();
    let attachments = collect_attachments(vault_root)?;
    Ok(plan_from_notes(&notes, &attachments, &old_clean, &new_clean))
}

/// Applies `edits` to the note at `rel_path` and saves it. Returns what changed and how to undo it.
pub fn apply_edits_to_note(vault_root: &Path, rel_path: &str, edits: &[TextEdit]) -> Result<AppliedEdits, String> {
    let (abs_path, clean) = resolve_safe_vault_path(vault_root, &ensure_md_extension(rel_path))?;
    let content = fs::read_to_string(&abs_path).map_err(|e| format!("Cannot read '{clean}': {e}"))?;
    let (updated, result) = apply_edits_to_string(&content, edits);
    if result.applied > 0 {
        atomic_write_str(&abs_path, &updated)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(p, c)| (p.to_string(), c.to_string())).collect()
    }

    /// Applies the plan and returns each changed note's new text, keyed by path.
    fn rewritten(all: &[(&str, &str)], old: &str, new: &str) -> Vec<(String, String)> {
        let plan = plan_from_notes(&notes(all), &[], old, new);
        plan.notes
            .iter()
            .map(|note| {
                let original = all
                    .iter()
                    .find(|(p, _)| *p == note.path || (*p == old && note.path == new))
                    .unwrap()
                    .1;
                (note.path.clone(), apply_edits_to_string(original, &note.edits).0)
            })
            .collect()
    }

    #[test]
    fn renames_wiki_links_and_keeps_their_alias_heading_and_embed() {
        let result = rewritten(
            &[
                ("Old.md", "Self [[Old]]"),
                ("A.md", "See [[Old|alias]], [[old#Head]] and ![[ Old ]]. [[Other]]"),
                ("B.md", "[[Other]]"),
            ],
            "Old.md",
            "New.md",
        );
        assert_eq!(
            result,
            vec![
                ("A.md".to_string(), "See [[New|alias]], [[New#Head]] and ![[ New ]]. [[Other]]".to_string()),
                ("New.md".to_string(), "Self [[New]]".to_string()),
            ]
        );
    }

    #[test]
    fn keeps_a_bare_name_through_a_move_and_changes_path_links() {
        let all = [
            ("Dup.md", "x"),
            ("A.md", "[[Dup]] [[Dup.md]]"),
            ("Docs/Guide.md", "[[Docs/Guide]] [[Guide]]"),
        ];
        // Nothing else is called Dup, so the bare name still finds it after the move.
        assert!(rewritten(&all, "Dup.md", "Archive/Dup.md").is_empty());
        // The path form follows the note, keeping or dropping `.md` as written.
        let result = rewritten(&all, "Docs/Guide.md", "Notes/Guide.md");
        assert_eq!(result, vec![("Notes/Guide.md".to_string(), "[[Notes/Guide]] [[Guide]]".to_string())]);
    }

    #[test]
    fn switches_to_a_path_when_a_move_makes_the_bare_name_ambiguous() {
        let all = [("Dup.md", "x"), ("X/Dup.md", "y"), ("A.md", "[[Dup]]")];
        let result = rewritten(&all, "Dup.md", "Deep/Er/Dup.md");
        assert_eq!(result, vec![("A.md".to_string(), "[[Deep/Er/Dup]]".to_string())]);
    }

    #[test]
    fn rewrites_markdown_links_relative_to_the_note_and_keeps_sections() {
        let all = [
            ("Docs/Old.md", "text"),
            ("Notes/S.md", "[t](../Docs/Old.md#sec) [r](/Docs/Old.md) ![i](<../Docs/Old.md> \"Title\") [w](https://x.org/Old.md)"),
            ("Docs/Sib.md", "[s](Old.md) [n](Old)"),
        ];
        let result = rewritten(&all, "Docs/Old.md", "Docs/New Name.md");
        assert_eq!(
            result,
            vec![
                (
                    "Docs/Sib.md".to_string(),
                    "[s](New%20Name.md) [n](New%20Name)".to_string()
                ),
                (
                    "Notes/S.md".to_string(),
                    "[t](../Docs/New%20Name.md#sec) [r](/Docs/New%20Name.md) ![i](<../Docs/New Name.md> \"Title\") [w](https://x.org/Old.md)".to_string()
                ),
            ]
        );
    }

    #[test]
    fn a_moved_note_keeps_its_own_relative_links_working() {
        let notes = notes(&[
            ("A/n.md", "[o](other.md) ![i](img/p.png) [r](/A/other.md) [[other]] [gone](nope.md)"),
            ("A/other.md", "[back](n.md)"),
        ]);
        let attachments = ["A/img/p.png".to_string()];
        let plan = plan_from_notes(&notes, &attachments, "A/n.md", "B/C/n.md");
        let moved = plan.notes.iter().find(|n| n.path == "B/C/n.md").unwrap();
        let (text, _) = apply_edits_to_string(&notes[0].1, &moved.edits);
        assert_eq!(text, "[o](../../A/other.md) ![i](../../A/img/p.png) [r](/A/other.md) [[other]] [gone](nope.md)");
        let other = plan.notes.iter().find(|n| n.path == "A/other.md").unwrap();
        assert_eq!(apply_edits_to_string(&notes[1].1, &other.edits).0, "[back](../B/C/n.md)");
        // A rename inside the same folder leaves the note's own links alone.
        let same = plan_from_notes(&notes, &attachments, "A/n.md", "A/m.md");
        assert!(same.notes.iter().all(|n| n.path != "A/m.md"));
    }

    #[test]
    fn ignores_code_properties_and_unrelated_links() {
        let all = [
            ("Old.md", "x"),
            ("A.md", "---\nrelated: \"[[Old]]\"\n---\n```\n[[Old]]\n```\n`[[Old]]` ``a ` [[Old]]`` \\[[Old]] [[Older]]\n\n    [[Old]] indented code\n~~~\n```\n[[Old]]\n~~~"),
        ];
        assert!(rewritten(&all, "Old.md", "New.md").is_empty());
    }

    #[test]
    fn offsets_count_utf16_units_and_each_line_break_once() {
        let all = [("Old.md", "x"), ("A.md", "😀 [[Old]]\r\n[[Old]]")];
        let plan = plan_from_notes(&notes(&all), &[], "Old.md", "New.md");
        let edits = &plan.notes[0].edits;
        assert_eq!((edits[0].from, edits[0].to), (5, 8));
        assert_eq!((edits[1].from, edits[1].to), (13, 16));
        let (updated, applied) = apply_edits_to_string(all[1].1, edits);
        assert_eq!(updated, "😀 [[New]]\r\n[[New]]");
        // The reverse edits restore the note exactly.
        let (restored, _) = apply_edits_to_string(&updated, &applied.reverse);
        assert_eq!(restored, all[1].1);
    }

    #[test]
    fn skips_edits_whose_text_changed_and_reports_the_count() {
        let plan = plan_from_notes(&notes(&[("Old.md", "x"), ("A.md", "[[Old]] and [[Old]]")]), &[], "Old.md", "New.md");
        let (updated, applied) = apply_edits_to_string("[[Old]] and [[Zzz]]", &plan.notes[0].edits);
        assert_eq!(updated, "[[New]] and [[Zzz]]");
        assert_eq!((applied.applied, applied.skipped), (1, 1));
    }

    #[test]
    fn plans_and_applies_edits_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Old.md"), "# Old").unwrap();
        fs::write(dir.path().join("A.md"), "Link [[Old]]").unwrap();
        let plan = plan_link_updates(dir.path(), "Old.md", "New").unwrap();
        assert_eq!(plan.link_count, 1);
        assert_eq!(plan.notes[0].path, "A.md");
        let applied = apply_edits_to_note(dir.path(), "A.md", &plan.notes[0].edits).unwrap();
        assert_eq!(applied.applied, 1);
        assert_eq!(fs::read_to_string(dir.path().join("A.md")).unwrap(), "Link [[New]]");
        // A folder has no links to update.
        fs::create_dir(dir.path().join("Folder")).unwrap();
        assert!(plan_link_updates(dir.path(), "Folder", "Other").unwrap().notes.is_empty());
    }
}
