//! Passage vectors for "Ask your vault", kept in one SQLite file per vault in the app data
//! folder, never in the vault. Each note records the modified time and embedding identity it was
//! embedded with, so a re-index sends only changed notes to the model.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection};

use crate::semantic::{Chunk, StoredChunk};

pub struct VectorStore {
    conn: Connection,
}

/// What a re-index must do to bring the store in line with the vault.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncPlan {
    /// Notes that are new, changed, or embedded with another provider, endpoint, or model.
    pub stale: Vec<String>,
    /// Stored notes that no longer exist in the vault.
    pub removed: Vec<String>,
}

fn db_err(context: &str, e: rusqlite::Error) -> String {
    format!("{context}: {e}")
}

fn encode(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn decode(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

impl VectorStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create the vector folder: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| db_err("Cannot open the vector store", e))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(|e| db_err("Cannot open the vector store", e))?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS notes (
                 path TEXT PRIMARY KEY,
                 modified_ms INTEGER NOT NULL,
                 model TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS chunks (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL REFERENCES notes(path) ON DELETE CASCADE,
                 heading TEXT NOT NULL,
                 line INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 vector BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chunks_path ON chunks(path);",
        )
        .map_err(|e| db_err("Cannot prepare the vector store", e))?;
        Ok(Self { conn })
    }

    /// Compares the vault's notes with the serialized embedding identity in `model`.
    pub fn plan(&self, vault_notes: &[(String, i64)], model: &str) -> Result<SyncPlan, String> {
        let mut stored: HashMap<String, (i64, String)> = HashMap::new();
        let mut statement = self
            .conn
            .prepare("SELECT path, modified_ms, model FROM notes")
            .map_err(|e| db_err("Cannot read the vector store", e))?;
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| db_err("Cannot read the vector store", e))?;
        for row in rows {
            let (path, modified, note_model) = row.map_err(|e| db_err("Cannot read the vector store", e))?;
            stored.insert(path, (modified, note_model));
        }

        let present: HashSet<&str> = vault_notes.iter().map(|(p, _)| p.as_str()).collect();
        let mut plan = SyncPlan::default();
        for (path, modified) in vault_notes {
            match stored.get(path) {
                Some((stored_modified, stored_model)) if stored_modified == modified && stored_model == model => {}
                _ => plan.stale.push(path.clone()),
            }
        }
        plan.removed = stored.keys().filter(|p| !present.contains(p.as_str())).cloned().collect();
        plan.removed.sort();
        Ok(plan)
    }

    /// Replaces everything stored for one note in a single transaction.
    pub fn replace_note(&mut self, path: &str, modified_ms: i64, model: &str, chunks: &[StoredChunk]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(|e| db_err("Cannot write the vector store", e))?;
        tx.execute("DELETE FROM notes WHERE path = ?1", [path]).map_err(|e| db_err("Cannot write the vector store", e))?;
        tx.execute("INSERT INTO notes (path, modified_ms, model) VALUES (?1, ?2, ?3)", params![path, modified_ms, model])
            .map_err(|e| db_err("Cannot write the vector store", e))?;
        for stored in chunks {
            tx.execute(
                "INSERT INTO chunks (path, heading, line, text, vector) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![path, stored.chunk.heading, stored.chunk.line as i64, stored.chunk.text, encode(&stored.vector)],
            )
            .map_err(|e| db_err("Cannot write the vector store", e))?;
        }
        tx.commit().map_err(|e| db_err("Cannot write the vector store", e))
    }

    pub fn remove_notes(&mut self, paths: &[String]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(|e| db_err("Cannot write the vector store", e))?;
        for path in paths {
            tx.execute("DELETE FROM notes WHERE path = ?1", [path]).map_err(|e| db_err("Cannot write the vector store", e))?;
        }
        tx.commit().map_err(|e| db_err("Cannot write the vector store", e))
    }

    /// Every stored passage matching the serialized embedding identity in `model`.
    pub fn load(&self, model: &str) -> Result<Vec<StoredChunk>, String> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT c.path, c.heading, c.line, c.text, c.vector FROM chunks c
                 JOIN notes n ON n.path = c.path WHERE n.model = ?1 ORDER BY c.id",
            )
            .map_err(|e| db_err("Cannot read the vector store", e))?;
        let rows = statement
            .query_map([model], |r| {
                Ok(StoredChunk {
                    chunk: Chunk {
                        path: r.get(0)?,
                        heading: r.get(1)?,
                        line: r.get::<_, i64>(2)? as usize,
                        text: r.get(3)?,
                    },
                    vector: decode(&r.get::<_, Vec<u8>>(4)?),
                })
            })
            .map_err(|e| db_err("Cannot read the vector store", e))?;
        rows.collect::<Result<_, _>>().map_err(|e| db_err("Cannot read the vector store", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(path: &str, text: &str, vector: Vec<f32>) -> StoredChunk {
        StoredChunk { chunk: Chunk { path: path.into(), heading: "H".into(), line: 3, text: text.into() }, vector }
    }

    fn notes(items: &[(&str, i64)]) -> Vec<(String, i64)> {
        items.iter().map(|(p, m)| (p.to_string(), *m)).collect()
    }

    #[test]
    fn vectors_survive_a_roundtrip() {
        let mut store = VectorStore::open_in_memory().unwrap();
        store.replace_note("A.md", 10, "m1", &[stored("A.md", "hello", vec![0.5, -1.25])]).unwrap();
        let loaded = store.load("m1").unwrap();
        assert_eq!(loaded, vec![stored("A.md", "hello", vec![0.5, -1.25])]);
        assert!(store.load("other-model").unwrap().is_empty());
    }

    #[test]
    fn the_plan_lists_new_changed_and_removed_notes() {
        let mut store = VectorStore::open_in_memory().unwrap();
        store.replace_note("Same.md", 1, "m1", &[stored("Same.md", "a", vec![1.0])]).unwrap();
        store.replace_note("Edited.md", 1, "m1", &[stored("Edited.md", "b", vec![1.0])]).unwrap();
        store.replace_note("Gone.md", 1, "m1", &[stored("Gone.md", "c", vec![1.0])]).unwrap();

        let plan = store.plan(&notes(&[("Same.md", 1), ("Edited.md", 2), ("New.md", 1)]), "m1").unwrap();
        assert_eq!(plan.stale, ["Edited.md", "New.md"]);
        assert_eq!(plan.removed, ["Gone.md"]);
    }

    #[test]
    fn changing_the_model_makes_every_note_stale() {
        let mut store = VectorStore::open_in_memory().unwrap();
        store.replace_note("A.md", 1, "m1", &[stored("A.md", "a", vec![1.0])]).unwrap();
        let plan = store.plan(&notes(&[("A.md", 1)]), "m2").unwrap();
        assert_eq!(plan.stale, ["A.md"]);
    }

    #[test]
    fn replacing_a_note_drops_its_old_passages() {
        let mut store = VectorStore::open_in_memory().unwrap();
        store.replace_note("A.md", 1, "m1", &[stored("A.md", "old", vec![1.0]), stored("A.md", "older", vec![1.0])]).unwrap();
        store.replace_note("A.md", 2, "m1", &[stored("A.md", "new", vec![1.0])]).unwrap();
        let texts: Vec<String> = store.load("m1").unwrap().into_iter().map(|s| s.chunk.text).collect();
        assert_eq!(texts, ["new"]);
        store.remove_notes(&["A.md".to_string()]).unwrap();
        assert!(store.load("m1").unwrap().is_empty());
    }
}
