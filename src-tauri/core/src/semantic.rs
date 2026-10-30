//! The retrieval half of "Ask your vault": split notes into passages, then rank stored passage
//! vectors against a question vector. Producing the vectors (a call to an embedding model) lives
//! in the app, so this module has no network code and its tests need no model.

use serde::{Deserialize, Serialize};

/// Longest passage, in characters. Longer sections split at paragraph breaks.
pub const MAX_CHUNK_CHARS: usize = 1200;

/// A passage of one note, ready to embed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub path: String,
    /// The heading trail above the passage, such as `Plan > Budget`. Empty before the first heading.
    pub heading: String,
    /// 1-based line where the passage starts.
    pub line: usize,
    pub text: String,
}

impl Chunk {
    /// The text sent to the embedding model: the note title and heading give a short passage
    /// enough context to match a question about it.
    pub fn embedding_input(&self, title: &str) -> String {
        if self.heading.is_empty() {
            format!("{title}\n\n{}", self.text)
        } else {
            format!("{title} > {}\n\n{}", self.heading, self.text)
        }
    }
}

/// Splits a note into passages at headings, then at paragraph breaks when a section exceeds
/// [`MAX_CHUNK_CHARS`]. Frontmatter and blank passages are dropped.
pub fn chunk_note(path: &str, content: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut trail: Vec<(usize, String)> = Vec::new();
    let mut section: Vec<(usize, &str)> = Vec::new();
    let mut in_fence = false;
    let mut lines = content.lines().enumerate().peekable();

    if matches!(lines.peek(), Some((_, "---"))) {
        // Skip YAML frontmatter up to its closing fence.
        let rest: Vec<_> = content.lines().collect();
        if let Some(end) = rest.iter().skip(1).position(|l| *l == "---") {
            for _ in 0..=end + 1 {
                lines.next();
            }
        }
    }

    let heading_of = |trail: &[(usize, String)]| trail.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > ");

    for (index, line) in lines {
        let fence = line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
        if fence {
            in_fence = !in_fence;
        }
        let level = if in_fence || fence { 0 } else { heading_level(line) };
        if level > 0 {
            flush(path, &heading_of(&trail), &mut section, &mut chunks);
            trail.retain(|(l, _)| *l < level);
            trail.push((level, line.trim_start_matches('#').trim().to_string()));
        } else {
            section.push((index + 1, line));
        }
    }
    flush(path, &heading_of(&trail), &mut section, &mut chunks);
    chunks
}

fn heading_level(line: &str) -> usize {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') { hashes } else { 0 }
}

/// Turns the collected lines of one section into one or more chunks and clears them.
fn flush(path: &str, heading: &str, section: &mut Vec<(usize, &str)>, out: &mut Vec<Chunk>) {
    let Some((mut line, _)) = section.first().copied() else { return };
    let text = section.drain(..).map(|(_, text)| text).collect::<Vec<_>>().join("\n");
    let mut remaining = text.as_str();
    while !remaining.is_empty() {
        let cap = remaining.char_indices().nth(MAX_CHUNK_CHARS).map_or(remaining.len(), |(i, _)| i);
        let end = if cap == remaining.len() {
            cap
        } else {
            let window = &remaining[..cap];
            window.rfind("\n\n").map(|i| i + 2)
                .or_else(|| window.rfind('\n').map(|i| i + 1))
                .or_else(|| window.rfind(' ').map(|i| i + 1))
                .unwrap_or(cap)
        };
        let piece = &remaining[..end];
        let trimmed = piece.trim();
        if !trimmed.is_empty() {
            let leading = piece.len() - piece.trim_start().len();
            let start = line + piece[..leading].bytes().filter(|b| *b == b'\n').count();
            out.push(Chunk { path: path.to_string(), heading: heading.to_string(), line: start, text: trimmed.to_string() });
        }
        line += piece.bytes().filter(|b| *b == b'\n').count();
        remaining = &remaining[end..];
    }
}

/// A stored passage with its vector.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredChunk {
    pub chunk: Chunk,
    pub vector: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub chunk: Chunk,
    pub score: f32,
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na.sqrt() * nb.sqrt()) }
}

/// The `limit` stored passages closest to `query`, best first. A vector of a different length
/// (from another embedding model) scores zero and is left out.
pub fn top_matches(stored: &[StoredChunk], query: &[f32], limit: usize) -> Vec<Match> {
    let mut scored: Vec<Match> = stored
        .iter()
        .filter(|s| s.vector.len() == query.len())
        .map(|s| Match { chunk: s.chunk.clone(), score: cosine(&s.vector, query) })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_follow_headings_and_keep_the_trail() {
        let note = "Intro text\n\n# Plan\nTop line\n\n## Budget\nNumbers here\n\n## Risks\nBad things\n";
        let chunks = chunk_note("Plan.md", note);
        let headings: Vec<&str> = chunks.iter().map(|c| c.heading.as_str()).collect();
        assert_eq!(headings, ["", "Plan", "Plan > Budget", "Plan > Risks"]);
        assert_eq!(chunks[2].text, "Numbers here");
        assert_eq!(chunks[2].line, 7);
    }

    #[test]
    fn frontmatter_and_empty_sections_are_dropped() {
        let note = "---\ntags: [a]\n---\n# Empty\n# Real\nBody\n";
        let chunks = chunk_note("N.md", note);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading, "Real");
        assert!(!chunks[0].text.contains("tags"));
    }

    #[test]
    fn a_hash_line_inside_a_code_fence_is_not_a_heading() {
        let note = "# Code\n```\n# not a heading\n```\n";
        let chunks = chunk_note("C.md", note);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("# not a heading"));
    }

    #[test]
    fn long_sections_split_at_paragraph_breaks() {
        let paragraph = "word ".repeat(150);
        let note = format!("# Long\n{paragraph}\n\n{paragraph}\n\n{paragraph}\n");
        let chunks = chunk_note("L.md", &note);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|c| c.heading == "Long"));
    }

    #[test]
    fn matches_rank_by_cosine_and_skip_other_dimensions() {
        let chunk = |t: &str| Chunk { path: "A.md".into(), heading: String::new(), line: 1, text: t.into() };
        let stored = vec![
            StoredChunk { chunk: chunk("far"), vector: vec![0.0, 1.0] },
            StoredChunk { chunk: chunk("near"), vector: vec![1.0, 0.1] },
            StoredChunk { chunk: chunk("other model"), vector: vec![1.0, 0.0, 0.0] },
        ];
        let found = top_matches(&stored, &[1.0, 0.0], 5);
        assert_eq!(found.iter().map(|m| m.chunk.text.as_str()).collect::<Vec<_>>(), ["near", "far"]);
    }

    #[test]
    fn long_unbroken_unicode_paragraphs_are_bounded_without_losing_text() {
        let body = "é🦀界".repeat(2000);
        let chunks = chunk_note("Unicode.md", &format!("# Heading\r\n{body}"));
        assert_eq!(chunks.len(), 5);
        assert!(chunks.iter().all(|c| c.text.chars().count() <= MAX_CHUNK_CHARS && c.line == 2));
        assert_eq!(chunks.iter().map(|c| c.text.as_str()).collect::<String>(), body);
    }

    #[test]
    fn long_code_blocks_preserve_source_lines_and_heading_context() {
        let code = (0..500).map(|i| format!("value_{i} = 12345;")).collect::<Vec<_>>();
        let note = format!("---\r\ntitle: Test\r\n---\r\n# Code\r\n```\r\n{}\r\n```\r\n", code.join("\r\n"));
        let chunks = chunk_note("Code.md", &note);
        let lines = note.lines().collect::<Vec<_>>();
        assert!(chunks.len() > 1);
        for chunk in chunks {
            assert!(chunk.text.chars().count() <= MAX_CHUNK_CHARS);
            assert_eq!(chunk.heading, "Code");
            assert_eq!(chunk.text.lines().next().unwrap(), lines[chunk.line - 1]);
        }
        assert!(chunk_note("Properties.md", "---\r\ntitle: Only properties\r\n---\r\n").is_empty());
    }
}
