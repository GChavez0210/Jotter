use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, QueryParser, RegexQuery};
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::index::ScannedNoteFile;
use crate::types::{MatchRange, SearchHit};

/// Full-text search index powered by `tantivy`.
///
/// Architectural properties:
/// 1. Keeps a single long-lived `IndexWriter` behind a `Mutex<IndexWriter>` reused across
///    `rebuild_from_scanned`, `upsert_scanned_note`, `upsert_batch`, and `delete_path_or_prefix`.
///    A second writer is never opened.
/// 2. Indexes a `path_prefix` `STRING` field with one token for the exact path plus one token per
///    ancestor folder so `delete_path_or_prefix` deletes by term without scanning segments or
///    reading stored documents.
/// 3. Stores each note's body in the index, compressed with LZ4, so a query builds its snippets
///    and match ranges from indexed content. A query never reads a Markdown file, and the
///    snippet always agrees with the text that was ranked.
/// 4. Exposes `title`, `body`, `tag`, and `path` as query fields (`tag:project`, `path:daily`).
///    Plain words search `title` and `body`, with title matches boosted.
/// 5. Cancels a running search when a newer one starts, so a slow query never delays the next.
pub struct VaultSearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    /// The exact vault path, for deletes and for returning hits.
    rel_path_field: Field,
    path_prefix_field: Field,
    /// The path split into words, for `path:` queries.
    path_field: Field,
    title_field: Field,
    tag_field: Field,
    body_field: Field,
    /// Counts searches. A search that sees a higher count than its own has been superseded.
    latest_search: AtomicU64,
}

/// A word a query looks for, used to find match ranges in a note's text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HighlightTerm {
    text: String,
    /// Edits a word may differ by and still match.
    max_edits: u8,
    /// Whether a longer word that starts with `text` matches.
    prefix: bool,
}

/// The longest snippet, in characters, and how many characters of it come before the first match.
const SNIPPET_CHARS: usize = 180;
const SNIPPET_LEAD: usize = 60;
/// Counting stops here so a huge note with a common word cannot slow a query.
const MAX_COUNTED_MATCHES: usize = 1000;

impl VaultSearchIndex {
    /// Creates a new in-RAM Tantivy full-text search index with a single long-lived `IndexWriter`.
    pub fn new() -> Result<Self, String> {
        let mut schema_builder = Schema::builder();
        let rel_path_field = schema_builder.add_text_field("rel_path", STRING | STORED);
        let path_prefix_field = schema_builder.add_text_field("path_prefix", STRING);
        let path_field = schema_builder.add_text_field("path", TEXT);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        let tag_field = schema_builder.add_text_field("tag", TEXT);
        // The body is stored, compressed, so snippets come from the index and not from the disk.
        let body_field = schema_builder.add_text_field("body", TEXT | STORED);
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
            rel_path_field,
            path_prefix_field,
            path_field,
            title_field,
            tag_field,
            body_field,
            latest_search: AtomicU64::new(0),
        })
    }

    fn build_tantivy_doc(&self, f: &ScannedNoteFile) -> TantivyDocument {
        let norm_path = normalize_search_rel_path(&f.path);
        let mut document = TantivyDocument::default();
        document.add_text(self.rel_path_field, &norm_path);
        document.add_text(self.path_prefix_field, &norm_path);
        document.add_text(self.path_field, &norm_path);

        // Index one token per ancestor folder so folder-prefix deletes are a single term delete
        for ancestor in ancestor_folder_prefixes(&norm_path) {
            document.add_text(self.path_prefix_field, &ancestor);
        }

        document.add_text(self.title_field, &f.title);
        for tag in &f.tags {
            document.add_text(self.tag_field, tag);
        }
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
        self.apply_changes(&[], files)
    }

    pub fn apply_changes(
        &self,
        deletions: &[String],
        files: &[ScannedNoteFile],
    ) -> Result<(), String> {
        if files.is_empty() && deletions.is_empty() {
            return Ok(());
        }
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| format!("Failed to lock Tantivy writer for upsert: {e}"))?;

        for path in deletions {
            let path = normalize_search_rel_path(path);
            if path.is_empty() {
                continue;
            }
            writer.delete_term(Term::from_field_text(self.rel_path_field, &path));
            writer.delete_term(Term::from_field_text(self.path_prefix_field, &path));
        }

        for f in files {
            let norm_path = normalize_search_rel_path(&f.path);
            writer.delete_term(Term::from_field_text(self.rel_path_field, &norm_path));
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

        writer.delete_term(Term::from_field_text(self.rel_path_field, &norm_path));
        writer.delete_term(Term::from_field_text(self.path_prefix_field, &norm_path));

        writer
            .commit()
            .map_err(|e| format!("Failed to commit Tantivy delete for {norm_path}: {e}"))?;
        self.reader
            .reload()
            .map_err(|e| format!("Failed to reload Tantivy reader after delete: {e}"))?;
        Ok(())
    }

    /// Searches the vault for `query_str` and returns up to `limit` hits. Each hit has a snippet
    /// and the ranges to highlight in it, both built from the indexed text. A search that a newer
    /// search overtakes before it finishes returns [`SEARCH_CANCELLED`].
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        let mine = self.latest_search.fetch_add(1, Ordering::SeqCst) + 1;
        self.search_while(query_str, limit, &|| {
            self.latest_search.load(Ordering::SeqCst) != mine
        })
    }

    /// Like [`Self::search`], and stops with [`SEARCH_CANCELLED`] once `cancelled` returns true.
    pub fn search_while(
        &self,
        query_str: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<SearchHit>, String> {
        let trimmed = query_str.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let (query, terms) = self.build_query(trimmed);

        let top_docs = searcher
            .search(
                &query,
                &TopDocs::with_limit(limit.clamp(1, 100)).order_by_score(),
            )
            .map_err(|e| format!("Tantivy search failed: {e}"))?;

        let mut analyzer = self
            .index
            .tokenizer_for_field(self.body_field)
            .map_err(|e| format!("Failed to load the search tokenizer: {e}"))?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (_score, doc_address) in top_docs {
            if cancelled() {
                return Err(SEARCH_CANCELLED.to_string());
            }
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| format!("Failed to load matched Tantivy doc: {e}"))?;
            let text_of = |field: Field| {
                doc.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let path = text_of(self.rel_path_field);
            let title = text_of(self.title_field);
            let body = text_of(self.body_field);
            hits.push(build_hit(&mut analyzer, path, title, &body, &terms));
        }
        Ok(hits)
    }
}

/// The error text of a search that a newer search overtook.
pub const SEARCH_CANCELLED: &str = "search-cancelled";

impl VaultSearchIndex {
    /// Parses `query_str` as a Tantivy query and returns the words to highlight.
    ///
    /// Queries with operators (`AND`, `OR`, `NOT`, `+`, `-`), parentheses, quoted phrases, or
    /// field names (`title:`, `body:`, `tag:`, `path:`) run exactly as written. Phrases accept
    /// `~N` for slop and a trailing `*` for a prefix, as in `"quick bro"*`.
    ///
    /// Plain word queries are forgiving. They also match words within a small edit distance, so
    /// "recieve" finds "receive", and the last word matches as a prefix, so "proj" finds
    /// "project" while the user is still typing. Exact matches get a boost and rank first.
    fn build_query(&self, query_str: &str) -> (Box<dyn Query>, Vec<HighlightTerm>) {
        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![self.title_field, self.body_field],
        );
        query_parser.set_field_boost(self.title_field, 2.0);
        let (exact, _errors) = query_parser.parse_query_lenient(query_str);

        if uses_query_syntax(query_str) {
            // Separate terms all have to match, so `path:projects bank` narrows the search.
            let mut query_parser = QueryParser::for_index(
                &self.index,
                vec![self.title_field, self.body_field],
            );
            query_parser.set_field_boost(self.title_field, 2.0);
            query_parser.set_conjunction_by_default();
            let (exact, _errors) = query_parser.parse_query_lenient(query_str);
            let mut terms: Vec<HighlightTerm> = Vec::new();
            exact.query_terms(&mut |term, _| {
                let field = term.field();
                if field != self.title_field && field != self.body_field && field != self.tag_field {
                    return;
                }
                if let Some(text) = term.value().as_str() {
                    let candidate = HighlightTerm {
                        text: text.to_string(),
                        max_edits: 0,
                        prefix: false,
                    };
                    if !terms.contains(&candidate) {
                        terms.push(candidate);
                    }
                }
            });
            return (exact, terms);
        }

        let words = self.analyzed_terms(query_str);
        let last = words.len().saturating_sub(1);
        let mut terms = Vec::with_capacity(words.len());
        let mut clauses: Vec<(Occur, Box<dyn Query>)> =
            vec![(Occur::Should, Box::new(BoostQuery::new(exact, 4.0)))];
        for (index, word) in words.iter().enumerate() {
            let distance = typo_distance(word);
            // The last word may be unfinished. Words of one letter would match too much.
            let prefix = index == last && word.chars().count() >= 2;
            terms.push(HighlightTerm {
                text: word.clone(),
                max_edits: distance,
                prefix,
            });
            for field in [self.title_field, self.body_field] {
                if distance > 0 {
                    let term = Term::from_field_text(field, word);
                    clauses.push((
                        Occur::Should,
                        Box::new(FuzzyTermQuery::new(term, distance, true)),
                    ));
                }
                // Tokens are alphanumeric, so the word needs no regex escaping.
                if prefix && word.chars().all(char::is_alphanumeric) {
                    if let Ok(regex) = RegexQuery::from_pattern(&format!("{word}.*"), field) {
                        clauses.push((Occur::Should, Box::new(regex)));
                    }
                }
            }
        }
        (Box::new(BooleanQuery::new(clauses)), terms)
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
    query.contains(['"', ':', '(', ')', '~', '^'])
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

/// Whether an indexed word satisfies a query word.
fn token_matches(token: &str, term: &HighlightTerm) -> bool {
    token == term.text
        || (term.prefix && token.starts_with(&term.text))
        || (term.max_edits > 0
            && edit_distance(token, &term.text, usize::from(term.max_edits))
                <= usize::from(term.max_edits))
}

/// Byte ranges of the words in `text` that match any term. The words come from the same
/// tokenizer that built the index, so a range always covers a word the index knows. Stops once
/// `cap` ranges are found.
fn find_matches(
    analyzer: &mut TextAnalyzer,
    text: &str,
    terms: &[HighlightTerm],
    cap: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    if terms.is_empty() {
        return ranges;
    }
    let mut stream = analyzer.token_stream(text);
    while stream.advance() {
        let token = stream.token();
        if terms.iter().any(|term| token_matches(&token.text, term)) {
            ranges.push((token.offset_from, token.offset_to));
            if ranges.len() >= cap {
                break;
            }
        }
    }
    ranges
}

/// The snippet's text and the byte ranges to highlight in it.
struct Snippet {
    text: String,
    ranges: Vec<(usize, usize)>,
}

/// Builds the snippet for the line of `body` that holds byte `anchor`. A line longer than
/// [`SNIPPET_CHARS`] is cut to a window around the anchor and marked with `…`. `matches` are
/// byte ranges in `body`; the ones inside the window come back as byte ranges in the snippet.
fn build_snippet(body: &str, anchor: usize, matches: &[(usize, usize)]) -> Snippet {
    let line_start = body[..anchor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = body[anchor..].find('\n').map_or(body.len(), |i| anchor + i);
    let line = &body[line_start..line_end];
    let start = line_start + (line.len() - line.trim_start().len());
    let end = line_start + line.trim_end().len();
    let (start, end) = if start <= anchor && anchor <= end { (start, end) } else { (line_start, line_end) };
    let text = &body[start..end];

    let total_chars = text.chars().count();
    let (window_start, window_end, cut_before, cut_after) = if total_chars <= SNIPPET_CHARS {
        (start, end, false, false)
    } else {
        let anchor_char = body[start..anchor.clamp(start, end)].chars().count();
        let to_char = (anchor_char.saturating_sub(SNIPPET_LEAD) + SNIPPET_CHARS).min(total_chars);
        let from_char = to_char.saturating_sub(SNIPPET_CHARS);
        let byte_of = |n: usize| {
            text.char_indices().nth(n).map_or(text.len(), |(i, _)| i)
        };
        (
            start + byte_of(from_char),
            start + byte_of(to_char),
            from_char > 0,
            to_char < total_chars,
        )
    };

    let lead = if cut_before { "…" } else { "" };
    let mut snippet = String::with_capacity(window_end - window_start + 6);
    snippet.push_str(lead);
    snippet.push_str(&body[window_start..window_end]);
    if cut_after {
        snippet.push('…');
    }
    let shift = lead.len();
    let ranges = matches
        .iter()
        .filter(|(from, _)| *from >= window_start && *from < window_end)
        .map(|&(from, to)| (from - window_start + shift, to.min(window_end) - window_start + shift))
        .collect();
    Snippet { text: snippet, ranges }
}

/// Converts a byte offset in `text` to a UTF-16 code unit offset, the unit JavaScript strings
/// index by. Rust offsets are UTF-8 bytes, so a range sent to the frontend must be converted:
/// `"é"` is two bytes but one unit, and `"😀"` is four bytes and two units.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn render_html(snippet: &Snippet) -> String {
    let mut html = String::with_capacity(snippet.text.len() + snippet.ranges.len() * 13);
    let mut at = 0;
    for &(from, to) in &snippet.ranges {
        if from < at {
            continue;
        }
        html.push_str(&escape_html_text(&snippet.text[at..from]));
        html.push_str("<mark>");
        html.push_str(&escape_html_text(&snippet.text[from..to]));
        html.push_str("</mark>");
        at = to;
    }
    html.push_str(&escape_html_text(&snippet.text[at..]));
    html
}

fn build_hit(
    analyzer: &mut TextAnalyzer,
    path: String,
    title: String,
    body: &str,
    terms: &[HighlightTerm],
) -> SearchHit {
    let matches = find_matches(analyzer, body, terms, MAX_COUNTED_MATCHES);
    let (match_count, anchor) = match matches.first() {
        Some(&(from, _)) => (matches.len(), Some(from)),
        None => (find_matches(analyzer, &title, terms, MAX_COUNTED_MATCHES).len(), None),
    };
    // With no match in the body, show the note's first line. The title or the path matched.
    let anchor = anchor.or_else(|| {
        body.char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map(|(i, _)| i)
    });

    let (snippet, line_number) = match anchor {
        Some(anchor) => (
            build_snippet(body, anchor, &matches),
            1 + body.as_bytes()[..anchor].iter().filter(|&&b| b == b'\n').count(),
        ),
        None => (Snippet { text: title.clone(), ranges: Vec::new() }, 1),
    };

    let snippet_html = render_html(&snippet);
    let ranges = snippet
        .ranges
        .iter()
        .map(|&(from, to)| MatchRange {
            start: utf16_offset(&snippet.text, from),
            end: utf16_offset(&snippet.text, to),
        })
        .collect();
    SearchHit {
        path,
        title,
        snippet_html,
        snippet: snippet.text,
        matches: ranges,
        line_number,
        match_count: match_count.max(1),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn scanned(path: &str, title: &str, content: &str, tags: &[&str]) -> ScannedNoteFile {
        ScannedNoteFile {
            path: path.to_string(),
            title: title.to_string(),
            fs_modified_ms: 1_700_000_000_000,
            created_ms: 1_700_000_000_000,
            preview: content.lines().next().unwrap_or_default().to_string(),
            content: content.to_string(),
            links: Vec::new(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    fn index_of(files: &[ScannedNoteFile]) -> VaultSearchIndex {
        let idx = VaultSearchIndex::new().expect("create tantivy index");
        idx.rebuild_from_scanned(files).unwrap();
        idx
    }

    fn paths(hits: &[SearchHit]) -> Vec<&str> {
        let mut paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        paths.sort_unstable();
        paths
    }

    /// The text a hit highlights, read the way JavaScript would: by UTF-16 offsets.
    fn highlighted(hit: &SearchHit) -> Vec<String> {
        let units: Vec<u16> = hit.snippet.encode_utf16().collect();
        hit.matches
            .iter()
            .map(|m| String::from_utf16(&units[m.start..m.end]).unwrap())
            .collect()
    }

    #[test]
    fn search_returns_highlighted_snippets_and_updates_incrementally() {
        let search_idx = index_of(&[
            scanned(
                "Specs/Sub/Clipboard.md",
                "Clipboard Engine",
                "# Clipboard\nPreserves exact Notepad line breaks in Slack and Word.",
                &[],
            ),
            scanned(
                "Daily/2026-10-19.md",
                "2026-10-19",
                "Worked on Windows 11 window controls and Fluent icons.",
                &[],
            ),
        ]);

        let hits = search_idx.search("Notepad", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Specs/Sub/Clipboard.md");
        assert_eq!(
            hits[0].snippet_html,
            "Preserves exact <mark>Notepad</mark> line breaks in Slack and Word."
        );
        assert_eq!(highlighted(&hits[0]), vec!["Notepad"]);
        assert_eq!(hits[0].line_number, 2);

        search_idx
            .upsert_scanned_note(&scanned(
                "Daily/2026-10-19.md",
                "2026-10-19",
                "Verified Notepad paste behavior in Google Chat.",
                &[],
            ))
            .unwrap();
        assert_eq!(search_idx.search("Notepad", 10).unwrap().len(), 2);

        // Deleting the ancestor folder "Specs" goes through the indexed path_prefix field.
        search_idx.delete_path_or_prefix("Specs").unwrap();
        let hits = search_idx.search("Notepad", 10).unwrap();
        assert_eq!(paths(&hits), vec!["Daily/2026-10-19.md"]);
    }

    #[test]
    fn snippets_come_from_the_index_and_never_from_a_file() {
        // The notes exist only in the index: no file is written anywhere.
        let search_idx = index_of(&[scanned("Ghost.md", "Ghost", "Line one\nThe unicorn appears here", &[])]);
        let hits = search_idx.search("unicorn", 10).unwrap();
        assert_eq!(hits[0].snippet, "The unicorn appears here");
        assert_eq!(hits[0].line_number, 2);
    }

    #[test]
    fn match_ranges_are_utf16_offsets_that_survive_non_ascii_text() {
        // "é" is two bytes and one UTF-16 unit; "日本語" is nine bytes and three units; the
        // emoji is four bytes and two units. A byte offset would slice the wrong text.
        let body = "café 日本語 😀 target and Target again, über target";
        let hit = &index_of(&[scanned("U.md", "U", body, &[])]).search("target", 10).unwrap()[0];
        assert_eq!(hit.snippet, body);
        assert_eq!(highlighted(hit), vec!["target", "Target", "target"]);
        assert_eq!(hit.match_count, 3);
        // The first range starts after 13 UTF-16 units, not after the 22 bytes before it.
        assert_eq!(hit.matches[0].start, "café 日本語 😀 ".encode_utf16().count());
        assert_ne!(hit.matches[0].start, "café 日本語 😀 ".len());
        assert!(hit.snippet_html.contains("<mark>target</mark>"));
    }

    #[test]
    fn a_long_line_is_cut_around_the_first_match() {
        let filler = "lorem ipsum dolor sit amet ".repeat(30);
        let body = format!("{filler}needle {filler}");
        let hit = &index_of(&[scanned("Long.md", "Long", &body, &[])]).search("needle", 10).unwrap()[0];
        assert!(hit.snippet.starts_with('…') && hit.snippet.ends_with('…'), "{}", hit.snippet);
        assert!(hit.snippet.chars().count() <= SNIPPET_CHARS + 2);
        assert_eq!(highlighted(hit), vec!["needle"]);
        // The window keeps the lead-in before the match.
        assert!(hit.matches[0].start > 30);
    }

    #[test]
    fn html_in_a_note_is_escaped_in_the_snippet() {
        let hit = &index_of(&[scanned("H.md", "H", "<script>alert(1)</script> marker & more", &[])])
            .search("marker", 10)
            .unwrap()[0];
        assert_eq!(
            hit.snippet_html,
            "&lt;script&gt;alert(1)&lt;/script&gt; <mark>marker</mark> &amp; more"
        );
    }

    #[test]
    fn typo_in_query_still_finds_the_note_and_ranks_exact_matches_first() {
        let search_idx = index_of(&[
            scanned("Meeting.md", "Meeting", "We receive the budget on Friday.", &[]),
            scanned("Typo.md", "Typo", "I always recieve mail late.", &[]),
        ]);

        let hits = search_idx.search("recieve", 10).unwrap();
        let order: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(order, vec!["Typo.md", "Meeting.md"]);
        assert_eq!(highlighted(&hits[1]), vec!["receive"]);

        let hits = search_idx.search("budgte", 10).unwrap();
        assert_eq!(paths(&hits), vec!["Meeting.md"]);
    }

    #[test]
    fn the_last_word_matches_as_a_prefix_while_typing() {
        let search_idx = index_of(&[
            scanned("A.md", "A", "The project plan is ready.", &[]),
            scanned("B.md", "B", "Nothing relevant here.", &[]),
        ]);
        let hits = search_idx.search("proj", 10).unwrap();
        assert_eq!(paths(&hits), vec!["A.md"]);
        assert_eq!(highlighted(&hits[0]), vec!["project"]);
        // Only the last word is a prefix, and one letter is too short to expand.
        assert!(search_idx.search("proj plan x", 10).unwrap().len() <= 1);
        assert!(search_idx.search("p", 10).unwrap().is_empty());
        // A trailing star is the same request.
        assert_eq!(paths(&search_idx.search("proj*", 10).unwrap()), vec!["A.md"]);
    }

    #[test]
    fn short_words_and_query_syntax_stay_exact() {
        let search_idx = index_of(&[
            scanned("Cat.md", "Cat", "The cat sat.", &[]),
            scanned("Receive.md", "Receive", "receive", &[]),
        ]);
        assert!(search_idx.search("cot", 10).unwrap().is_empty());
        assert!(search_idx.search("\"recieve\"", 10).unwrap().is_empty());
        assert!(search_idx.search("recei AND cat", 10).unwrap().is_empty());
    }

    #[test]
    fn boolean_operators_and_phrases() {
        let search_idx = index_of(&[
            scanned("Both.md", "Both", "apples and oranges grow here", &[]),
            scanned("Apples.md", "Apples", "only apples here", &[]),
            scanned("Oranges.md", "Oranges", "just oranges there", &[]),
        ]);
        assert_eq!(paths(&search_idx.search("apples AND oranges", 10).unwrap()), vec!["Both.md"]);
        assert_eq!(
            paths(&search_idx.search("apples OR oranges", 10).unwrap()),
            vec!["Apples.md", "Both.md", "Oranges.md"]
        );
        assert_eq!(paths(&search_idx.search("apples NOT oranges", 10).unwrap()), vec!["Apples.md"]);
        assert_eq!(paths(&search_idx.search("apples -oranges", 10).unwrap()), vec!["Apples.md"]);
        assert_eq!(paths(&search_idx.search("+apples +oranges", 10).unwrap()), vec!["Both.md"]);
        assert_eq!(paths(&search_idx.search("(apples OR oranges) AND grow", 10).unwrap()), vec!["Both.md"]);
        assert_eq!(paths(&search_idx.search("\"oranges grow\"", 10).unwrap()), vec!["Both.md"]);
        assert!(search_idx.search("\"grow oranges\"", 10).unwrap().is_empty());
        // A phrase with a trailing star matches a longer last word.
        assert_eq!(paths(&search_idx.search("\"oranges gr\"*", 10).unwrap()), vec!["Both.md"]);
        // The phrase highlights each of its words.
        let hit = &search_idx.search("\"apples and\"", 10).unwrap()[0];
        assert_eq!(highlighted(hit), vec!["apples", "and"]);
    }

    #[test]
    fn field_filters_narrow_by_title_tag_path_and_body() {
        let search_idx = index_of(&[
            scanned("Daily/Mon.md", "Monday", "Call the bank today", &["work", "project/alpha"]),
            scanned("Projects/Bank.md", "Bank", "Monday standup notes", &["finance"]),
        ]);
        assert_eq!(paths(&search_idx.search("title:monday", 10).unwrap()), vec!["Daily/Mon.md"]);
        assert_eq!(paths(&search_idx.search("body:monday", 10).unwrap()), vec!["Projects/Bank.md"]);
        assert_eq!(paths(&search_idx.search("monday", 10).unwrap()), vec!["Daily/Mon.md", "Projects/Bank.md"]);
        assert_eq!(paths(&search_idx.search("tag:finance", 10).unwrap()), vec!["Projects/Bank.md"]);
        assert_eq!(paths(&search_idx.search("tag:#work", 10).unwrap()), vec!["Daily/Mon.md"]);
        // A parent tag finds nested tags, and the whole nested tag finds only that tag.
        assert_eq!(paths(&search_idx.search("tag:project", 10).unwrap()), vec!["Daily/Mon.md"]);
        assert_eq!(paths(&search_idx.search("tag:project/alpha", 10).unwrap()), vec!["Daily/Mon.md"]);
        assert_eq!(paths(&search_idx.search("path:daily", 10).unwrap()), vec!["Daily/Mon.md"]);
        assert_eq!(paths(&search_idx.search("path:projects bank", 10).unwrap()), vec!["Projects/Bank.md"]);
        // Filters combine with terms and operators.
        assert_eq!(
            paths(&search_idx.search("monday AND tag:work", 10).unwrap()),
            vec!["Daily/Mon.md"]
        );
        assert!(search_idx.search("tag:finance AND path:daily", 10).unwrap().is_empty());
    }

    #[test]
    fn title_matches_rank_above_body_matches() {
        let search_idx = index_of(&[
            scanned("Body.md", "Misc", "mentions budget once", &[]),
            scanned("Title.md", "Budget", "other text entirely", &[]),
        ]);
        let hits = search_idx.search("budget", 10).unwrap();
        assert_eq!(hits[0].path, "Title.md");
        // A title-only hit still shows the note's first line, with no highlights.
        assert_eq!(hits[0].snippet, "other text entirely");
        assert!(hits[0].matches.is_empty());
        assert_eq!(hits[0].match_count, 1);
    }

    #[test]
    fn a_newer_search_cancels_the_one_it_overtakes() {
        let files: Vec<ScannedNoteFile> = (0..50)
            .map(|i| scanned(&format!("N{i}.md"), &format!("N{i}"), "shared word", &[]))
            .collect();
        let search_idx = index_of(&files);
        // The callback stands in for a newer search arriving after the first hit.
        let calls = std::cell::Cell::new(0);
        let result = search_idx.search_while("shared", 50, &|| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        });
        assert_eq!(result, Err(SEARCH_CANCELLED.to_string()));
        // An uncontested search is unaffected.
        assert_eq!(search_idx.search("shared", 50).unwrap().len(), 50);
    }

    #[test]
    fn edit_distance_counts_transpositions_once() {
        assert_eq!(edit_distance("recieve", "receive", 2), 1);
        assert_eq!(edit_distance("kitten", "sitting", 5), 3);
        assert_eq!(edit_distance("abc", "abcdefgh", 2), 3);
    }

    #[test]
    fn concurrent_search_index_upserts_from_two_threads_do_not_drop_updates() {
        let search_idx = Arc::new(VaultSearchIndex::new().expect("create tantivy index"));

        let per_thread_count = 20usize;
        let mut handles = Vec::new();

        for thread_id in 0..2 {
            let idx_clone = Arc::clone(&search_idx);
            handles.push(std::thread::spawn(move || {
                for i in 0..per_thread_count {
                    let rel_path = format!("Thread{thread_id}/Note_{i}.md");
                    let title = format!("Thread {thread_id} Note {i}");
                    let content =
                        format!("# {title}\nConcurrentSharedKeyword thread_{thread_id}_item_{i}");
                    idx_clone
                        .upsert_scanned_note(&scanned(&rel_path, &title, &content, &[]))
                        .expect("concurrent upsert must succeed without dropping");
                }
            }));
        }

        for h in handles {
            h.join().expect("worker thread panicked");
        }

        let all_hits = search_idx
            .search("ConcurrentSharedKeyword", per_thread_count * 2 + 10)
            .expect("search after concurrent upserts");

        assert_eq!(
            all_hits.len(),
            per_thread_count * 2,
            "All {} concurrent upserts across both threads must be present in the index",
            per_thread_count * 2
        );
    }

    /// Timing for the acceptance target: common queries under 100 ms at p95 on 10,000 notes.
    /// Run it with `cargo test -p jotter-core --release search_benchmark -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark: run on demand"]
    fn search_benchmark_on_ten_thousand_notes() {
        use std::time::Instant;
        const WORDS: [&str; 24] = [
            "meeting", "project", "budget", "design", "review", "travel", "recipe", "garden",
            "network", "backup", "invoice", "roadmap", "release", "customer", "research",
            "planning", "journal", "reading", "workout", "kitchen", "weekend", "deadline",
            "feedback", "archive",
        ];
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let files: Vec<ScannedNoteFile> = (0..10_000)
            .map(|i| {
                let mut body = String::new();
                for line in 0..40 {
                    for _ in 0..12 {
                        body.push_str(WORDS[(next() % 24) as usize]);
                        body.push(' ');
                    }
                    body.push_str(&format!("line{line}\n"));
                }
                scanned(
                    &format!("Folder{}/Note{i}.md", i % 50),
                    &format!("Note {i} {}", WORDS[(next() % 24) as usize]),
                    &body,
                    &["bench"],
                )
            })
            .collect();

        let build = Instant::now();
        let idx = index_of(&files);
        println!("indexed {} notes in {:?}", files.len(), build.elapsed());

        let queries = [
            "meeting", "project budget", "recieve", "proj", "\"design review\"",
            "budget AND NOT travel", "tag:bench garden", "path:folder7 backup", "title:invoice",
        ];
        for query in queries {
            let mut times: Vec<u128> = (0..30)
                .map(|_| {
                    let start = Instant::now();
                    idx.search(query, 40).unwrap();
                    start.elapsed().as_micros()
                })
                .collect();
            times.sort_unstable();
            println!(
                "{query:<24} median {:>7} us   p95 {:>7} us",
                times[times.len() / 2],
                times[times.len() * 95 / 100]
            );
        }
    }
}
