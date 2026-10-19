use std::fs;
use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, QueryParser};
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
        let query = self.build_query(trimmed);

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

impl VaultSearchIndex {
    /// Parses `query_str` as a Tantivy query. Plain word queries also match terms within a small
    /// edit distance, so "recieve" finds "receive". Exact matches get a boost and rank first.
    /// Queries with operators, field names, or quoted phrases run exactly as written.
    fn build_query(&self, query_str: &str) -> Box<dyn Query> {
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.title_field, self.body_field]);
        let (exact, _errors) = query_parser.parse_query_lenient(query_str);
        if uses_query_syntax(query_str) {
            return exact;
        }

        let mut clauses: Vec<(Occur, Box<dyn Query>)> =
            vec![(Occur::Should, Box::new(BoostQuery::new(exact, 4.0)))];
        for word in self.analyzed_terms(query_str) {
            let distance = typo_distance(&word);
            if distance == 0 {
                continue;
            }
            for field in [self.title_field, self.body_field] {
                let term = Term::from_field_text(field, &word);
                clauses.push((
                    Occur::Should,
                    Box::new(FuzzyTermQuery::new(term, distance, true)),
                ));
            }
        }
        Box::new(BooleanQuery::new(clauses))
    }

    /// Runs `text` through the body field's tokenizer, so fuzzy terms match indexed terms.
    fn analyzed_terms(&self, text: &str) -> Vec<String> {
        let Ok(mut analyzer) = self.index.tokenizer_for_field(self.body_field) else {
            return Vec::new();
        };
        let mut terms = Vec::new();
        let mut stream = analyzer.token_stream(text);
        while stream.advance() {
            let token = stream.token().text.clone();
            if !terms.contains(&token) {
                terms.push(token);
            }
        }
        terms
    }
}

fn uses_query_syntax(query: &str) -> bool {
    query.contains(['"', ':', '(', ')', '*', '~', '^'])
        || query
            .split_whitespace()
            .any(|w| w.starts_with(['+', '-']) || matches!(w, "AND" | "OR" | "NOT"))
}

/// Edits allowed for a typo match: none for short words, where one edit changes the word,
/// one for medium words, and two for long words.
fn typo_distance(word: &str) -> u8 {
    match word.chars().count() {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Levenshtein distance with adjacent transpositions counted as one edit, capped at `limit + 1`.
fn edit_distance(a: &str, b: &str, limit: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > limit {
        return limit + 1;
    }
    let mut prev2: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        prev2 = std::mem::replace(&mut prev, cur);
    }
    prev[b.len()].min(limit + 1)
}

/// Byte ranges in `line` of words that contain `token` or are a typo away from it.
fn token_matches_in_line(line: &str, token: &str) -> Vec<(usize, usize)> {
    let distance = usize::from(typo_distance(token));
    let check = |start: usize, end: usize| -> Option<(usize, usize)> {
        let word = line[start..end].to_lowercase();
        if let Some(idx) = word.find(token) {
            // Lowercasing keeps byte offsets only when the length is unchanged.
            if word.len() == end - start {
                return Some((start + idx, start + idx + token.len()));
            }
            return Some((start, end));
        }
        (distance > 0 && edit_distance(&word, token, distance) <= distance).then_some((start, end))
    };

    let mut ranges = Vec::new();
    let mut word_start: Option<usize> = None;
    for (idx, ch) in line.char_indices() {
        let in_word = ch.is_alphanumeric() || ch == '_' || ch == '-';
        match (in_word, word_start) {
            (true, None) => word_start = Some(idx),
            (false, Some(start)) => {
                ranges.extend(check(start, idx));
                word_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = word_start {
        ranges.extend(check(start, line.len()));
    }
    ranges
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
    for tok in tokens {
        if let Some(&(from, to)) = token_matches_in_line(&trimmed, tok).first() {
            return format!(
                "{}<mark>{}</mark>{}",
                escape_html_text(&trimmed[..from]),
                escape_html_text(&trimmed[from..to]),
                escape_html_text(&trimmed[to..])
            );
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
        let mut line_matched = false;
        for tok in tokens {
            let cnt = token_matches_in_line(line, tok).len();
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
        for tok in tokens {
            total_matches += token_matches_in_line(title, tok).len();
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
    fn typo_in_query_still_finds_the_note_and_ranks_exact_matches_first() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let search_idx = VaultSearchIndex::new().expect("create tantivy index");
        let docs = vec![
            sample_scanned_in_dir(root, "Meeting.md", "Meeting", "We receive the budget on Friday."),
            sample_scanned_in_dir(root, "Typo.md", "Typo", "I always recieve mail late."),
        ];
        search_idx.rebuild_from_scanned(&docs).unwrap();

        let hits = search_idx.search(Some(root), "recieve", 10).unwrap();
        let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["Typo.md", "Meeting.md"]);
        assert!(hits[1].snippet_html.contains("<mark>receive</mark>"), "{}", hits[1].snippet_html);

        let hits = search_idx.search(Some(root), "budgte", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Meeting.md");
    }

    #[test]
    fn short_words_and_query_syntax_stay_exact() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let search_idx = VaultSearchIndex::new().expect("create tantivy index");
        let docs = vec![
            sample_scanned_in_dir(root, "Cat.md", "Cat", "The cat sat."),
            sample_scanned_in_dir(root, "Receive.md", "Receive", "receive"),
        ];
        search_idx.rebuild_from_scanned(&docs).unwrap();

        assert!(search_idx.search(Some(root), "cot", 10).unwrap().is_empty());
        assert!(search_idx.search(Some(root), "\"recieve\"", 10).unwrap().is_empty());
    }

    #[test]
    fn edit_distance_counts_transpositions_once() {
        assert_eq!(edit_distance("recieve", "receive", 2), 1);
        assert_eq!(edit_distance("kitten", "sitting", 5), 3);
        assert_eq!(edit_distance("abc", "abcdefgh", 2), 3);
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
