//! Vault health report: broken links, attachments no note uses, empty notes, and duplicate titles.
//!
//! The report reads the vault from disk instead of the SQLite index. The index records only
//! `[[wiki links]]`, and Markdown links and image embeds decide whether an attachment is in use.

use crate::index::{normalize_link_target, ScannedNoteFile, WikiLinkResolver};
use crate::links::{parse_wiki_links_with_lines, split_yaml_frontmatter};
use crate::md_scan::{in_spans, inline_code_spans, prose_lines};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

/// The most entries each list carries. The totals stay exact, so the UI can say "and 40 more".
const LIST_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokenLink {
    pub source_path: String,
    pub line_number: usize,
    /// The link target as written, without alias or heading.
    pub target: String,
    /// `wiki` for `[[Target]]`, `markdown` for `[text](target)`.
    pub kind: String,
    pub line_context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateTitle {
    pub title: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthList<T> {
    pub total: usize,
    pub items: Vec<T>,
}

impl<T> HealthList<T> {
    fn new(mut items: Vec<T>) -> Self {
        let total = items.len();
        items.truncate(LIST_LIMIT);
        Self { total, items }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub broken_links: HealthList<BrokenLink>,
    pub unused_attachments: HealthList<String>,
    pub empty_notes: HealthList<String>,
    pub duplicate_titles: HealthList<DuplicateTitle>,
}

/// Scans `vault_root` and builds the report.
pub fn scan_vault_health(vault_root: &Path, templates_folder: &str) -> Result<HealthReport, String> {
    let notes = crate::index::scan_dir_files_from_disk(vault_root, vault_root)?;
    let attachments = collect_attachments(vault_root)?;
    Ok(build_report(&notes, &attachments, templates_folder))
}

/// The vault-relative path of every file that is not a note, outside hidden folders.
pub(crate) fn collect_attachments(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    collect_attachments_in(root, root, &mut out)?;
    Ok(out)
}

fn collect_attachments_in(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        if fs::symlink_metadata(&path)
            .map(|m| crate::vault::is_link_metadata(&m))
            .unwrap_or(true)
        {
            continue;
        }
        if path.is_dir() {
            collect_attachments_in(root, &path, out)?;
        } else if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    Ok(())
}

/// Builds the report from scanned notes and the vault-relative paths of every non-note file.
/// Notes inside `templates_folder` are left out of the broken, empty, and duplicate lists.
pub fn build_report(
    notes: &[ScannedNoteFile],
    attachments: &[String],
    templates_folder: &str,
) -> HealthReport {
    let templates_prefix = templates_folder.trim_matches('/').to_lowercase();
    let in_templates = |path: &str| {
        !templates_prefix.is_empty()
            && path.to_lowercase().starts_with(&format!("{templates_prefix}/"))
    };
    let note_paths: Vec<String> = notes.iter().map(|n| n.path.clone()).collect();
    let note_resolver = WikiLinkResolver::new(&note_paths);
    let attachment_resolver = WikiLinkResolver::new(attachments);

    // Every file a Markdown link can point at, keyed by lowercase path.
    let mut all_files: HashMap<String, String> = HashMap::new();
    for path in note_paths.iter().chain(attachments.iter()) {
        all_files.insert(path.to_lowercase(), path.clone());
    }

    let mut used: HashSet<String> = HashSet::new();
    let mut broken: Vec<BrokenLink> = Vec::new();

    for note in notes {
        // Templates hold placeholders such as `[[{{title}}]]`, so only their attachment uses count.
        let is_template = in_templates(&note.path);
        for link in parse_wiki_links_with_lines(&note.path, &note.content) {
            if note_resolver.resolve(&note.path, &link.target).is_some() {
                continue;
            }
            if let Some(file) = attachment_resolver.resolve(&note.path, &link.target) {
                used.insert(file);
                continue;
            }
            if is_template {
                continue;
            }
            broken.push(BrokenLink {
                source_path: note.path.clone(),
                line_number: link.line_number,
                target: link.target,
                kind: "wiki".into(),
                line_context: link.line_context,
            });
        }

        for link in parse_markdown_links(&note.content) {
            let Some(dest) = local_destination(&link.destination) else {
                continue;
            };
            match resolve_markdown_destination(&note.path, &dest, &all_files) {
                Some(file) => {
                    used.insert(file);
                }
                None if link.html || is_template => {}
                None => broken.push(BrokenLink {
                    source_path: note.path.clone(),
                    line_number: link.line_number,
                    target: dest,
                    kind: "markdown".into(),
                    line_context: link.line_context,
                }),
            }
        }
    }

    // A property such as `cover: attachments/photo.png` also keeps an attachment in use.
    let frontmatter: Vec<String> = notes
        .iter()
        .filter_map(|n| split_yaml_frontmatter(&n.content).1.map(str::to_lowercase))
        .collect();
    let mut unused: Vec<String> = attachments
        .iter()
        .filter(|path| !used.contains(*path))
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
            !frontmatter.iter().any(|fm| fm.contains(&name))
        })
        .cloned()
        .collect();
    unused.sort();

    let mut empty: Vec<String> = notes
        .iter()
        .filter(|n| !in_templates(&n.path) && is_empty_note(&n.content))
        .map(|n| n.path.clone())
        .collect();
    empty.sort();

    let mut by_title: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for note in notes.iter().filter(|n| !in_templates(&n.path)) {
        by_title
            .entry(normalize_link_target(&note.title))
            .or_default()
            .push((note.title.clone(), note.path.clone()));
    }
    let duplicates: Vec<DuplicateTitle> = by_title
        .into_values()
        .filter(|group| group.len() > 1)
        .map(|mut group| {
            group.sort_by(|a, b| a.1.cmp(&b.1));
            DuplicateTitle {
                title: group[0].0.clone(),
                paths: group.into_iter().map(|(_, path)| path).collect(),
            }
        })
        .collect();

    broken.sort_by(|a, b| {
        (a.source_path.as_str(), a.line_number).cmp(&(b.source_path.as_str(), b.line_number))
    });

    HealthReport {
        broken_links: HealthList::new(broken),
        unused_attachments: HealthList::new(unused),
        empty_notes: HealthList::new(empty),
        duplicate_titles: HealthList::new(duplicates),
    }
}

/// A note is empty when nothing follows its properties, or only a single heading does.
fn is_empty_note(content: &str) -> bool {
    let body = split_yaml_frontmatter(content).0;
    let mut lines = body.lines().map(str::trim).filter(|l| !l.is_empty());
    match (lines.next(), lines.next()) {
        (None, _) => true,
        (Some(first), None) => first.starts_with('#') && first.trim_start_matches('#').starts_with(' '),
        _ => false,
    }
}

struct MarkdownLink {
    destination: String,
    line_number: usize,
    line_context: String,
    /// A `src` or `href` attribute in inline HTML. It keeps a file in use but is not reported
    /// as broken, since HTML can point at things the vault does not hold.
    html: bool,
}

/// The value of each `src="…"` or `href="…"` attribute in an HTML tag on the line.
fn html_references(line: &str, code: &[std::ops::Range<usize>]) -> Vec<String> {
    let lower = line.to_ascii_lowercase();
    let mut refs = Vec::new();
    for attr in ["src=", "href="] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(attr) {
            let at = from + pos;
            from = at + attr.len();
            let inside_tag = lower[..at].rfind('<').is_some_and(|open| !lower[open..at].contains('>'));
            let starts_word = at > 0 && lower.as_bytes()[at - 1].is_ascii_whitespace();
            if !inside_tag || !starts_word || in_spans(code, at) {
                continue;
            }
            let Some(quote) = line[from..].chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            if let Some(end) = line[from + 1..].find(quote) {
                refs.push(line[from + 1..from + 1 + end].to_string());
            }
        }
    }
    refs
}

/// Finds `[text](destination)`, `![alt](destination)`, and HTML `src`/`href` values outside code
/// and properties.
fn parse_markdown_links(content: &str) -> Vec<MarkdownLink> {
    let (body, frontmatter) = split_yaml_frontmatter(content);
    let offset = frontmatter.map_or(0, |fm| {
        (if fm.is_empty() { 0 } else { fm.lines().count() }) + 2
    });
    let mut links = Vec::new();

    for prose in prose_lines(body) {
        let line = prose.text;
        let line_number = offset + prose.index + 1;
        let line_context: String = line.trim().chars().take(160).collect();
        let code = inline_code_spans(line);
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b']' && !in_spans(&code, i) && bytes.get(i + 1) == Some(&b'(') {
                let start = i + 2;
                if let Some(len) = destination_length(&line[start..]) {
                    links.push(MarkdownLink {
                        destination: line[start..start + len].to_string(),
                        line_number,
                        line_context: line_context.clone(),
                        html: false,
                    });
                    i = start + len;
                    continue;
                }
            }
            i += 1;
        }
        if line.contains('<') {
            for destination in html_references(line, &code) {
                links.push(MarkdownLink {
                    destination,
                    line_number,
                    line_context: line_context.clone(),
                    html: true,
                });
            }
        }
    }
    links
}

/// Length of the link destination in `rest`, which starts just after `(`. A destination ends at
/// the matching `)`, and may be wrapped in `<...>` to allow spaces.
pub(crate) fn destination_length(rest: &str) -> Option<usize> {
    if let Some(inner) = rest.strip_prefix('<') {
        let end = inner.find('>')?;
        let after = &inner[end + 1..];
        let close = after.find(')')?;
        return Some(1 + end + 1 + close);
    }
    let mut depth = 0usize;
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The vault-relative part of a link destination, or `None` for web links, anchors, and anything
/// else that does not point at a file.
pub(crate) fn local_destination(raw: &str) -> Option<String> {
    let mut dest = raw.trim();
    if let Some(inner) = dest.strip_prefix('<') {
        dest = inner.split('>').next()?;
    } else if let Some((path, _title)) = dest.split_once(char::is_whitespace) {
        dest = path;
    }
    let dest = dest.split('#').next()?.split('?').next()?.trim();
    if dest.is_empty() {
        return None;
    }
    // `scheme:` covers http, https, mailto, data, obsidian, and the rest. A drive letter is one letter.
    if let Some((scheme, _)) = dest.split_once(':') {
        if scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) {
            return None;
        }
    }
    if dest.starts_with("//") {
        return None;
    }
    Some(percent_decode(dest).replace('\\', "/"))
}

pub(crate) fn percent_decode(raw: &str) -> String {
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

/// Resolves a Markdown destination against the vault the way the image reader does: relative to
/// the note, then from the vault root, then inside `attachments/`. A note link may omit `.md`.
fn resolve_markdown_destination(
    source_path: &str,
    dest: &str,
    files: &HashMap<String, String>,
) -> Option<String> {
    let from_root = dest.trim_start_matches('/');
    let source_dir = source_path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let candidates = [
        join_normalized(source_dir, dest),
        join_normalized("", from_root),
        join_normalized("attachments", from_root),
    ];
    for candidate in candidates.into_iter().flatten() {
        for key in [candidate.clone(), format!("{candidate}.md")] {
            if let Some(found) = files.get(&key.to_lowercase()) {
                return Some(found.clone());
            }
        }
    }
    None
}

/// Joins `dir` and `rel`, resolving `.` and `..`. Returns `None` when the path leaves the vault.
pub(crate) fn join_normalized(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for segment in rel.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, content: &str) -> ScannedNoteFile {
        ScannedNoteFile {
            path: path.to_string(),
            title: path.rsplit('/').next().unwrap().trim_end_matches(".md").to_string(),
            fs_modified_ms: 0,
            created_ms: 0,
            preview: String::new(),
            content: content.to_string(),
            links: parse_wiki_links_with_lines(path, content),
            tags: Vec::new(),
        }
    }

    #[test]
    fn reports_broken_wiki_and_markdown_links_but_not_embeds() {
        let notes = [
            note(
                "Index.md",
                "See [[Missing note]] and [[Real]].\n![[photo.png]] ![alt](attachments/photo.png)\n[gone](Nope.md) [web](https://example.com) [top](#heading)",
            ),
            note("Real.md", "Body"),
        ];
        let report = build_report(&notes, &["attachments/photo.png".to_string()], "");
        let broken: Vec<_> = report
            .broken_links
            .items
            .iter()
            .map(|b| (b.target.as_str(), b.kind.as_str(), b.line_number))
            .collect();
        assert_eq!(broken, vec![("Missing note", "wiki", 1), ("Nope.md", "markdown", 3)]);
        assert!(report.unused_attachments.items.is_empty());
    }

    #[test]
    fn finds_unused_attachments_and_respects_properties() {
        let notes = [
            note("A.md", "---\ncover: shot.png\n---\n![[used.png]]"),
            note("Sub/B.md", "![x](../img/rel%20path.png)"),
        ];
        let attachments = [
            "used.png".to_string(),
            "shot.png".to_string(),
            "img/rel path.png".to_string(),
            "img/orphan.pdf".to_string(),
        ];
        let report = build_report(&notes, &attachments, "");
        assert_eq!(report.unused_attachments.items, vec!["img/orphan.pdf".to_string()]);
        assert!(report.broken_links.items.is_empty());
    }

    #[test]
    fn skips_templates_and_counts_html_references_as_uses() {
        let notes = [
            note("Templates/Meeting.md", "# {{title}}\n\n[[{{title}} notes]] ![[logo.png]]"),
            note("Templates/Copy.md", ""),
            note("A.md", "<img src=\"pics/a.png\" alt=\"x\"> <a href='doc.pdf'>d</a> <img src=\"gone.png\">\n`<img src=\"code.png\">`"),
            note("Meeting.md", "text"),
        ];
        let attachments = [
            "logo.png".to_string(),
            "pics/a.png".to_string(),
            "doc.pdf".to_string(),
            "code.png".to_string(),
        ];
        let report = build_report(&notes, &attachments, "Templates");
        assert!(report.broken_links.items.is_empty());
        assert!(report.empty_notes.items.is_empty());
        assert_eq!(report.duplicate_titles.total, 0);
        assert_eq!(report.unused_attachments.items, vec!["code.png".to_string()]);
    }

    #[test]
    fn flags_empty_notes_and_duplicate_titles() {
        let notes = [
            note("Blank.md", "---\ntags: [a]\n---\n\n"),
            note("Only title.md", "# Only title\n\n"),
            note("Real.md", "# Real\ntext"),
            note("One/Same.md", "text"),
            note("Two/same.md", "text"),
        ];
        let report = build_report(&notes, &[], "");
        assert_eq!(report.empty_notes.items, vec!["Blank.md", "Only title.md"]);
        assert_eq!(report.duplicate_titles.total, 1);
        assert_eq!(
            report.duplicate_titles.items[0].paths,
            vec!["One/Same.md", "Two/same.md"]
        );
    }

    #[test]
    fn ignores_links_in_code_and_caps_lists_with_exact_totals() {
        let mut content = String::from("```\n[x](nope.md)\n```\n`[y](nope2.md)`\n");
        for i in 0..(LIST_LIMIT + 5) {
            content.push_str(&format!("[l](missing{i}.md)\n"));
        }
        let report = build_report(&[note("A.md", &content)], &[], "");
        assert_eq!(report.broken_links.total, LIST_LIMIT + 5);
        assert_eq!(report.broken_links.items.len(), LIST_LIMIT);
    }

    #[test]
    fn destinations_handle_titles_angle_brackets_and_schemes() {
        assert_eq!(local_destination("a/b.png \"Title\"").as_deref(), Some("a/b.png"));
        assert_eq!(local_destination("<my file.png>").as_deref(), Some("my file.png"));
        assert_eq!(local_destination("note.md#section").as_deref(), Some("note.md"));
        assert_eq!(local_destination("mailto:a@b.c"), None);
        assert_eq!(local_destination("C:/docs/x.png").as_deref(), Some("C:/docs/x.png"));
        assert_eq!(local_destination("#top"), None);
        assert_eq!(join_normalized("a", "../../x"), None);
    }
}
