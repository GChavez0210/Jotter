use std::fs;
use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::index::ScannedNoteFile;
use crate::types::SearchHit;

/// Full-text search index powered by `tantivy`.
///
/// Architectural properties:
/// 1. Keeps a single long-lived `IndexWriter` behind a `Mutex<IndexWriter>` reused across
///    `rebuild_from_scanned`, `upsert_scanned_note`, `upsert_batch`, and `delete_path_or_prefix`.
///    A second writer is never opened.
/// 2. Indexes a `path_prefix` `STRING` field with one token for the exact path plus one token per
///    ancestor folder so `delete_path_or_prefix` deletes by term without scanning segments or
///    reading stored documents.
/// 3. Does NOT store full note bodies in `tantivy` (`body` is `TEXT` only, not `STORED`).
///    Highlighted `<mark>` snippets and 1-based line numbers are generated from the `.md` file
///    on disk for only the top `limit` hits returned by a query.
pub struct VaultSearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    path_field: Field,
    path_prefix_field: Field,
    title_field: Field,
    body_field: Field,
}

impl VaultSearchIndex {
    /// Creates a new in-RAM Tantivy full-text search index with a single long-lived `IndexWriter`.
    pub fn new() -> Result<Self, String> {
        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let path_prefix_field = schema_builder.add_text_field("path_prefix", STRING);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        // Full note body is indexed for full-text search but NOT stored in Tantivy RAM
        let body_field = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let writer = index
            .writer(15_000_000)
            .map_err(|e| format!("Failed to create long-lived Tantivy writer: {e}"))?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| format!("Failed to create Tantivy index reader: {e}"))?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            path_field,
            path_prefix_field,
            title_field,
            body_field,
        })
    }

    fn build_tantivy_doc(&self, f: &ScannedNoteFile) -> TantivyDocument {
        let norm_path = normalize_search_rel_path(&f.path);
        let mut document = TantivyDocument::default();
        document.add_text(self.path_field, &norm_path);
        document.add_text(self.path_prefix_field, &norm_path);

        // Index one token per ancestor folder so folder-prefix deletes are a single term delete
        for ancestor in ancestor_folder_prefixes(&norm_path) {
            document.add_text(self.path_prefix_field, &ancestor);
        }

        document.add_text(self.title_field, &f.title);
        document.add_text(self.body_field, &f.content);
        document
    }

    /// Rebuilds the entire Tantivy search index from pre-scanned vault files (called on vault open).
    pub fn rebuild_from_scanned(&self, files: &[ScannedNoteFile]) -> Result<usize, String> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| format!("Failed to lock Tantivy writer for rebuild: {e}"))?;

        writer
            .delete_all_documents()
            .map_err(|e| format!("Failed to clear Tantivy index: {e}"))?;

        for f in files {
            let doc = self.build_tantivy_doc(f);
            writer
                .add_document(doc)
                .map_err(|e| format!("Failed to add document {} to Tantivy: {e}", f.path))?;
        }

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy rebuild: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader after rebuild: {e}"))?;

        Ok(files.len())
    }

    /// Incrementally upserts a single scanned note in the Tantivy index using the shared `IndexWriter`.
    pub fn upsert_scanned_note(&self, scanned: &ScannedNoteFile) -> Result<(), String> {
        self.upsert_batch(std::slice::from_ref(scanned))
    }

    /// Incrementally upserts a batch of scanned notes in a single Tantivy commit.
    pub fn upsert_batch(&self, files: &[ScannedNoteFile]) -> Result<(), String> {
        if files.is_empty() {
            return Ok(());
        }
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| format!("Failed to lock Tantivy writer for upsert: {e}"))?;

        for f in files {
            let norm_path = normalize_search_rel_path(&f.path);
            writer.delete_term(Term::from_field_text(self.path_field, &norm_path));
            let doc = self.build_tantivy_doc(f);
            writer
                .add_document(doc)
                .map_err(|e| format!("Failed to add document {} to Tantivy: {e}", f.path))?;
        }

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy upsert: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader after upsert: {e}"))?;
        Ok(())
    }

    /// Incrementally deletes a note path or all notes under a folder prefix (`rel_path/...`)
    /// via the indexed `path_prefix` field without scanning segments or reading stored documents.
    pub fn delete_path_or_prefix(&self, rel_path: &str) -> Result<(), String> {
        let norm_path = normalize_search_rel_path(rel_path);
        if norm_path.is_empty() {
            return Ok(());
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|e| format!("Failed to lock Tantivy writer for delete: {e}"))?;

        writer.delete_term(Term::from_field_text(self.path_field, &norm_path));
        writer.delete_term(Term::from_field_text(self.path_prefix_field, &norm_path));

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy delete for {norm_path}: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader after delete: {e}"))?;
        Ok(())
    }

    /// Searches the vault for `query_str` and returns up to `limit` hits with `<mark>` highlighted snippets.
    /// Reads matched `.md` files from `vault_root` on disk so Tantivy does not need to store document bodies.
    pub fn search(
        &self,
        vault_root: Option<&Path>,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, String> {
        let trimmed = query_str.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.title_field, self.body_field]);
        let (query, _errors) = query_parser.parse_query_lenient(trimmed);

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit.max(1)))
            .map_err(|e| format!("Tantivy search failed: {e}"))?;

        let query_tokens: Vec<String> = trimmed
            .split_whitespace()
            .map(|t| {
                t.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty())
            .collect();

        let mut hits = Vec::with_capacity(top_docs.len());

        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| format!("Failed to load matched Tantivy doc: {e}"))?;

            let path = retrieved_doc
                .get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let title = retrieved_doc
                .get_first(self.title_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let disk_body = vault_root
                .and_then(|root| fs::read_to_string(root.join(&path)).ok())
                .unwrap_or_default();

            let (line_number, match_count, best_line) =
                compute_line_and_match_stats(&disk_body, &title, &query_tokens);

            let snippet_html = highlight_line_html(&best_line, &query_tokens);

            hits.push(SearchHit {
                path,
                title,
                snippet_html,
                line_number,
                match_count: match_count.max(1),
            });
        }

        Ok(hits)
    }
}

fn normalize_search_rel_path(rel_path: &str) -> String {
    rel_path
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

fn ancestor_folder_prefixes(norm_path: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    for (idx, ch) in norm_path.char_indices() {
        if ch == '/' && idx > 0 {
            prefixes.push(norm_path[..idx].to_string());
        }
    }
    prefixes
}

fn escape_html_text(input: &str) -> String {
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

fn highlight_line_html(line: &str, tokens: &[String]) -> String {
    let trimmed: String = line.trim().chars().take(180).collect();
    if tokens.is_empty() {
        return escape_html_text(&trimmed);
    }
    let lower = trimmed.to_lowercase();
    for tok in tokens {
        if let Some(idx) = lower.find(tok.as_str()) {
            let end = (idx + tok.len()).min(trimmed.len());
            if trimmed.is_char_boundary(idx) && trimmed.is_char_boundary(end) {
                return format!(
                    "{}<mark>{}</mark>{}",
                    escape_html_text(&trimmed[..idx]),
                    escape_html_text(&trimmed[idx..end]),
                    escape_html_text(&trimmed[end..])
                );
            }
        }
    }
    escape_html_text(&trimmed)
}

fn compute_line_and_match_stats(
    body: &str,
    title: &str,
    tokens: &[String],
) -> (usize, usize, String) {
    let mut first_line = 1usize;
    let mut found_first = false;
    let mut total_matches = 0usize;
    let mut first_line_text = String::new();

    for (idx, line) in body.lines().enumerate() {
        if first_line_text.is_empty() && !line.trim().is_empty() {
            first_line_text = line.trim().to_string();
        }
        let lower = line.to_lowercase();
        let mut line_matched = false;
        for tok in tokens {
            let cnt = lower.matches(tok.as_str()).count();
            if cnt > 0 {
                total_matches += cnt;
                line_matched = true;
            }
        }
        if line_matched && !found_first {
            found_first = true;
            first_line = idx + 1;
            first_line_text = line.trim().to_string();
        }
    }

    if total_matches == 0 {
        let lower_title = title.to_lowercase();
        for tok in tokens {
            total_matches += lower_title.matches(tok.as_str()).count();
        }
    }

    if first_line_text.is_empty() {
        first_line_text = title.to_string();
    }

    (first_line, total_matches, first_line_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn sample_scanned_in_dir(
        root: &Path,
        path: &str,
        title: &str,
        content: &str,
    ) -> ScannedNoteFile {
        let full_path = root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();

        ScannedNoteFile {
            path: path.to_string(),
            title: title.to_string(),
            fs_modified_ms: 1_700_000_000_000,
            created_ms: 1_700_000_000_000,
            preview: content.lines().next().unwrap_or_default().to_string(),
            content: content.to_string(),
            links: Vec::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn tantivy_search_returns_highlighted_snippets_and_updates_incrementally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();

        let search_idx = VaultSearchIndex::new().expect("create tantivy index");
        let docs = vec![
            sample_scanned_in_dir(
                root,
                "Specs/Sub/Clipboard.md",
                "Clipboard Engine",
                "# Clipboard\nPreserves exact Notepad line breaks in Slack and Word.",
            ),
            sample_scanned_in_dir(
                root,
                "Daily/2026-10-19.md",
                "2026-10-19",
                "Worked on Windows 11 Mica backdrop and Fluent icons.",
            ),
        ];

        let count = search_idx.rebuild_from_scanned(&docs).unwrap();
        assert_eq!(count, 2);

        let hits = search_idx.search(Some(root), "Notepad", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Specs/Sub/Clipboard.md");
        assert!(
            hits[0].snippet_html.contains("<mark>Notepad</mark>")
                || hits[0].snippet_html.to_lowercase().contains("<mark>notepad</mark>"),
            "Expected <mark> highlight in snippet: {}",
            hits[0].snippet_html
        );
        assert_eq!(hits[0].line_number, 2);

        // Incrementally update Daily/2026-10-19.md to also mention Notepad
        let updated = sample_scanned_in_dir(
            root,
            "Daily/2026-10-19.md",
            "2026-10-19",
            "Verified Notepad paste behavior in Google Chat.",
        );
        search_idx.upsert_scanned_note(&updated).unwrap();

        let hits_after_update = search_idx.search(Some(root), "Notepad", 10).unwrap();
        assert_eq!(hits_after_update.len(), 2);

        // Incrementally delete ancestor folder "Specs" via indexed path_prefix without scanning docs
        search_idx.delete_path_or_prefix("Specs").unwrap();
        let hits_after_delete = search_idx.search(Some(root), "Notepad", 10).unwrap();
        assert_eq!(hits_after_delete.len(), 1);
        assert_eq!(hits_after_delete[0].path, "Daily/2026-10-19.md");
    }

    #[test]
    fn concurrent_search_index_upserts_from_two_threads_do_not_drop_updates() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let search_idx = Arc::new(VaultSearchIndex::new().expect("create tantivy index"));

        let per_thread_count = 20usize;
        let mut handles = Vec::new();

        for thread_id in 0..2 {
            let idx_clone = Arc::clone(&search_idx);
            let root_clone = root.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..per_thread_count {
                    let rel_path = format!("Thread{thread_id}/Note_{i}.md");
                    let title = format!("Thread {thread_id} Note {i}");
                    let content = format!(
                        "# {title}\nConcurrentSharedKeyword thread_{thread_id}_item_{i}"
                    );
                    let scanned =
                        sample_scanned_in_dir(&root_clone, &rel_path, &title, &content);
                    idx_clone
                        .upsert_scanned_note(&scanned)
                        .expect("concurrent upsert must succeed without dropping");
                }
            }));
        }

        for h in handles {
            h.join().expect("worker thread panicked");
        }

        let all_hits = search_idx
            .search(
                Some(&root),
                "ConcurrentSharedKeyword",
                per_thread_count * 2 + 10,
            )
            .expect("search after concurrent upserts");

        assert_eq!(
            all_hits.len(),
            per_thread_count * 2,
            "All {} concurrent upserts across both threads must be present in the index",
            per_thread_count * 2
        );
    }
}
