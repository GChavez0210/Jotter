use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index, IndexReader, ReloadPolicy, TantivyDocument, Term};

use crate::index::ScannedNoteFile;
use crate::types::SearchHit;

/// Full-text search index powered by `tantivy`, rebuilt on vault open and updated incrementally.
pub struct VaultSearchIndex {
    index: Index,
    reader: IndexReader,
    path_field: Field,
    title_field: Field,
    body_field: Field,
}

impl VaultSearchIndex {
    /// Creates a new in-RAM Tantivy full-text search index for the vault.
    pub fn new() -> Result<Self, String> {
        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| format!("Failed to create Tantivy index reader: {e}"))?;

        Ok(Self {
            index,
            reader,
            path_field,
            title_field,
            body_field,
        })
    }

    /// Rebuilds the entire Tantivy search index from pre-scanned vault files (called on vault open).
    pub fn rebuild_from_scanned(&self, files: &[ScannedNoteFile]) -> Result<usize, String> {
        let mut writer = self
            .index
            .writer(15_000_000)
            .map_err(|e| format!("Failed to acquire Tantivy writer: {e}"))?;

        writer
            .delete_all_documents()
            .map_err(|e| format!("Failed to clear Tantivy index: {e}"))?;

        for f in files {
            writer
                .add_document(doc!(
                    self.path_field => f.path.as_str(),
                    self.title_field => f.title.as_str(),
                    self.body_field => f.content.as_str(),
                ))
                .map_err(|e| format!("Failed to add document {} to Tantivy: {e}", f.path))?;
        }

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy rebuild: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader: {e}"))?;

        Ok(files.len())
    }

    /// Incrementally upserts a single scanned note in the Tantivy index.
    pub fn upsert_scanned_note(&self, scanned: &ScannedNoteFile) -> Result<(), String> {
        self.upsert_batch(std::slice::from_ref(scanned))
    }

    /// Incrementally upserts a batch of scanned notes in a single Tantivy commit.
    pub fn upsert_batch(&self, files: &[ScannedNoteFile]) -> Result<(), String> {
        if files.is_empty() {
            return Ok(());
        }
        let mut writer = self
            .index
            .writer(15_000_000)
            .map_err(|e| format!("Failed to acquire Tantivy writer: {e}"))?;

        for f in files {
            let term = Term::from_field_text(self.path_field, &f.path);
            writer.delete_term(term);
            writer
                .add_document(doc!(
                    self.path_field => f.path.as_str(),
                    self.title_field => f.title.as_str(),
                    self.body_field => f.content.as_str(),
                ))
                .map_err(|e| format!("Failed to add document {} to Tantivy: {e}", f.path))?;
        }

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy upsert: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader: {e}"))?;
        Ok(())
    }

    /// Incrementally deletes a note path or all notes under a folder prefix (`rel_path/...`).
    pub fn delete_path_or_prefix(&self, rel_path: &str) -> Result<(), String> {
        let searcher = self.reader.searcher();
        let prefix = format!("{rel_path}/");
        let mut matching_paths = vec![rel_path.to_string()];

        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader
                .get_store_reader(1)
                .map_err(|e| format!("Failed to read Tantivy store: {e}"))?;
            for doc_id in segment_reader.doc_ids_alive() {
                if let Ok(doc) = store_reader.get::<TantivyDocument>(doc_id) {
                    if let Some(p) = doc.get_first(self.path_field).and_then(|v| v.as_str()) {
                        if p == rel_path || p.starts_with(&prefix) {
                            matching_paths.push(p.to_string());
                        }
                    }
                }
            }
        }

        let mut writer = self
            .index
            .writer::<TantivyDocument>(15_000_000)
            .map_err(|e| format!("Failed to acquire Tantivy writer: {e}"))?;
        for p in matching_paths {
            writer.delete_term(Term::from_field_text(self.path_field, &p));
        }
        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy delete: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader: {e}"))?;
        Ok(())
    }

    /// Searches the vault for `query_str` and returns up to `limit` hits with `<mark>` highlighted snippets.
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
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

        let mut snippet_gen = SnippetGenerator::create(&searcher, &*query, self.body_field)
            .map_err(|e| format!("Failed to create SnippetGenerator: {e}"))?;
        snippet_gen.set_max_num_chars(180);

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
            let body = retrieved_doc
                .get_first(self.body_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let (line_number, match_count, fallback_line) =
                compute_line_and_match_stats(body, &title, &query_tokens);

            let snippet = snippet_gen.snippet_from_doc(&retrieved_doc);
            let raw_html = snippet.to_html();
            let snippet_html = if !raw_html.trim().is_empty() {
                // Tantivy's Snippet::to_html() wraps highlights in `<b>...</b>`; convert to `<mark>`
                raw_html
                    .replace("<b>", "<mark>")
                    .replace("</b>", "</mark>")
            } else {
                highlight_line_html(&fallback_line, &query_tokens)
            };

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
    let trimmed: String = line.trim().chars().take(160).collect();
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

    fn sample_scanned(path: &str, title: &str, content: &str) -> ScannedNoteFile {
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
        let search_idx = VaultSearchIndex::new().expect("create tantivy index");
        let docs = vec![
            sample_scanned(
                "Specs/Clipboard.md",
                "Clipboard Engine",
                "# Clipboard\nPreserves exact Notepad line breaks in Slack and Word.",
            ),
            sample_scanned(
                "Daily/2026-10-19.md",
                "2026-10-19",
                "Worked on Windows 11 Mica backdrop and Fluent icons.",
            ),
        ];

        let count = search_idx.rebuild_from_scanned(&docs).unwrap();
        assert_eq!(count, 2);

        let hits = search_idx.search("Notepad", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Specs/Clipboard.md");
        assert!(
            hits[0].snippet_html.contains("<mark>Notepad</mark>")
                || hits[0].snippet_html.to_lowercase().contains("<mark>notepad</mark>"),
            "Expected <mark> highlight in snippet: {}",
            hits[0].snippet_html
        );
        assert_eq!(hits[0].line_number, 2);

        // Incrementally update Daily/2026-10-19.md to also mention Notepad
        let updated = sample_scanned(
            "Daily/2026-10-19.md",
            "2026-10-19",
            "Verified Notepad paste behavior in Google Chat.",
        );
        search_idx.upsert_scanned_note(&updated).unwrap();

        let hits_after_update = search_idx.search("Notepad", 10).unwrap();
        assert_eq!(hits_after_update.len(), 2);

        // Incrementally delete Specs folder
        search_idx.delete_path_or_prefix("Specs").unwrap();
        let hits_after_delete = search_idx.search("Notepad", 10).unwrap();
        assert_eq!(hits_after_delete.len(), 1);
        assert_eq!(hits_after_delete[0].path, "Daily/2026-10-19.md");
    }
}
