//! The operations the `jotter` command-line tool and its MCP server expose to agents.
//!
//! Each call works on the vault files directly and answers with JSON. Reads build a throwaway
//! index in memory from the files on disk, so they never touch the app's own index and never
//! fail because the app has the database open. Writes go through the same atomic write path the
//! app uses, keep the text they replace in the note's version history first, and send deletes
//! to the Recycle Bin. A read-only handle refuses every write.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::index::{self, system_time_to_ms};
use crate::search::VaultSearchIndex;
use crate::vault;
use crate::versions;

pub struct AgentVault {
    root: PathBuf,
    read_only: bool,
    versions_dir: PathBuf,
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| format!("Cannot encode the result: {e}"))
}

fn now_ms() -> i64 {
    system_time_to_ms(SystemTime::now())
}

impl AgentVault {
    /// Opens the vault at `root`. `app_data_dir` holds the version history, so a snapshot taken
    /// here appears in the app's timeline for the same note.
    pub fn open(root: &Path, read_only: bool, app_data_dir: &Path) -> Result<Self, String> {
        // Preserve the path spelling the app uses for history. Canonicalization can expand
        // Windows short names or macOS /var aliases into a different persistence key.
        let history_root = std::path::absolute(root)
            .map_err(|e| format!("Cannot resolve the vault '{}': {e}", root.display()))?;
        let versions_dir = versions::versions_dir(app_data_dir, &history_root);
        let root = root
            .canonicalize()
            .map_err(|e| format!("Cannot open the vault '{}': {e}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("'{}' is not a folder", root.display()));
        }
        Ok(Self { root, read_only, versions_dir })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn writable(&self) -> Result<(), String> {
        if self.read_only {
            Err("This vault is open read-only, so writes are refused".to_string())
        } else {
            Ok(())
        }
    }

    fn index(&self) -> Result<Connection, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("Cannot build the index: {e}"))?;
        let files = index::scan_dir_files_from_disk(&self.root, &self.root)?;
        index::apply_scanned_vault_index(&conn, &files)?;
        Ok(conn)
    }

    /// Keeps the text a write is about to replace, even when the last snapshot is recent.
    fn keep_previous(&self, rel_path: &str, previous: &str) {
        // A failed snapshot must never stop the write itself, as in the app.
        let _ = versions::snapshot(&self.versions_dir, rel_path, previous, now_ms(), 0);
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Value, String> {
        let files = index::scan_dir_files_from_disk(&self.root, &self.root)?;
        let search = VaultSearchIndex::new()?;
        search.rebuild_from_scanned(&files)?;
        let hits = search.search(Some(&self.root), query, limit.clamp(1, 200))?;
        to_json(&hits)
    }

    /// Every note with its metadata and link counts, optionally only those under `folder`.
    pub fn list(&self, folder: Option<&str>) -> Result<Value, String> {
        let mut cards = index::query_note_cards(&self.index()?)?;
        if let Some(folder) = folder {
            let folder = vault::normalize_rel_path(folder);
            if !folder.is_empty() {
                let (_, clean) = vault::resolve_safe_vault_path(&self.root, &folder)?;
                let prefix = format!("{clean}/");
                cards.retain(|card| card.path.starts_with(&prefix));
            }
        }
        to_json(&cards)
    }

    pub fn read(&self, path: &str) -> Result<Value, String> {
        to_json(&vault::read_vault_note(&self.root, &vault::ensure_md_extension(path))?)
    }

    pub fn create(&self, path: &str, content: &str) -> Result<Value, String> {
        self.writable()?;
        let (_, doc) = vault::create_vault_note(&self.root, path, content)?;
        to_json(&doc)
    }

    /// Adds `text` to the end of a note on its own line. With `create_missing`, a note that does
    /// not exist yet is created instead of reported as an error.
    pub fn append(&self, path: &str, text: &str, create_missing: bool) -> Result<Value, String> {
        self.writable()?;
        let rel = vault::ensure_md_extension(path);
        let (abs, clean) = vault::resolve_safe_vault_path(&self.root, &rel)?;
        if !abs.exists() {
            if !create_missing {
                return Err(format!("Note does not exist: {clean}"));
            }
            return self.create(&clean, &with_final_newline(text));
        }
        let previous = std::fs::read_to_string(&abs).map_err(|e| format!("Cannot read {clean}: {e}"))?;
        let separator = if previous.is_empty() || previous.ends_with('\n') { "" } else { "\n" };
        let updated = format!("{previous}{separator}{}", with_final_newline(text));
        self.keep_previous(&clean, &previous);
        let (doc, _) = vault::save_vault_note(&self.root, &clean, &updated)?;
        to_json(&doc)
    }

    /// Renames or moves a note or folder. For a note, links that point to it are rewritten
    /// unless `update_links` is false; each rewritten note keeps its old text in version history.
    pub fn rename(&self, from: &str, to: &str, update_links: bool) -> Result<Value, String> {
        self.writable()?;
        let from = &self.existing_entry(from)?;
        let plan = if update_links {
            crate::link_update::plan_link_updates(&self.root, from, to)?
        } else {
            Default::default()
        };
        let renamed = vault::rename_vault_entry(&self.root, from, to)?;
        versions::rename(&self.versions_dir, &renamed.old_rel_path, &renamed.new_rel_path)?;
        let mut notes_updated = 0;
        for note in &plan.notes {
            let (abs, clean) = vault::resolve_safe_vault_path(&self.root, &note.path)?;
            if let Ok(previous) = std::fs::read_to_string(&abs) {
                self.keep_previous(&clean, &previous);
            }
            if crate::link_update::apply_edits_to_note(&self.root, &clean, &note.edits)?.applied > 0 {
                notes_updated += 1;
            }
        }
        Ok(json!({
            "oldPath": renamed.old_rel_path,
            "newPath": renamed.new_rel_path,
            "linksUpdated": plan.link_count,
            "notesUpdated": notes_updated,
        }))
    }

    /// Sends a note or folder to the Recycle Bin.
    pub fn delete(&self, path: &str) -> Result<Value, String> {
        self.writable()?;
        let clean = self.existing_entry(path)?;
        vault::delete_vault_entry(&self.root, &clean)?;
        Ok(json!({ "deleted": clean }))
    }

    /// The vault-relative path of an existing note or folder. A name without `.md` finds the note.
    fn existing_entry(&self, path: &str) -> Result<String, String> {
        let (abs, clean) = vault::resolve_safe_vault_path(&self.root, path)?;
        if abs.exists() {
            return Ok(clean);
        }
        let (note_abs, note_clean) = vault::resolve_safe_vault_path(&self.root, &vault::ensure_md_extension(path))?;
        if note_abs.is_file() {
            Ok(note_clean)
        } else {
            Err(format!("Nothing exists at '{clean}'"))
        }
    }

    pub fn backlinks(&self, path: &str) -> Result<Value, String> {
        let (_, clean) = vault::resolve_safe_vault_path(&self.root, &vault::ensure_md_extension(path))?;
        to_json(&index::query_backlinks_for_note(&self.index()?, &clean)?)
    }

    /// All tags with their note counts, or the notes carrying `tag`.
    pub fn tags(&self, tag: Option<&str>) -> Result<Value, String> {
        let conn = self.index()?;
        match tag {
            Some(tag) => to_json(&index::query_notes_by_tag(&conn, tag.trim_start_matches('#'))?),
            None => to_json(&index::query_all_tags(&conn)?),
        }
    }

    /// Checklist items that have a due date.
    pub fn tasks(&self) -> Result<Value, String> {
        to_json(&index::query_due_tasks(&self.index()?)?)
    }

    pub fn health(&self) -> Result<Value, String> {
        to_json(&crate::health::scan_vault_health(&self.root, "")?)
    }

    /// The stored versions of a note, newest first.
    pub fn versions(&self, path: &str) -> Result<Value, String> {
        let (_, clean) = versions::resolve_note_path(&self.root, path)?;
        to_json(&versions::list(&self.versions_dir, &clean))
    }

    pub fn version(&self, path: &str, id: &str) -> Result<Value, String> {
        let (_, clean) = versions::resolve_note_path(&self.root, path)?;
        Ok(json!({ "path": clean, "id": id, "content": versions::read(&self.versions_dir, &clean, id)? }))
    }
}

fn with_final_newline(text: &str) -> String {
    if text.ends_with('\n') { text.to_string() } else { format!("{text}\n") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with_notes() -> (tempfile::TempDir, tempfile::TempDir) {
        let vault = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(vault.path().join("Sub")).unwrap();
        std::fs::write(vault.path().join("Home.md"), "# Home\nSee [[Plan]] #work\n- [ ] Pay 📅 2026-11-01\n").unwrap();
        std::fs::write(vault.path().join("Plan.md"), "The plan about oranges").unwrap();
        (vault, tempfile::tempdir().unwrap())
    }

    fn open(vault: &tempfile::TempDir, data: &tempfile::TempDir, read_only: bool) -> AgentVault {
        AgentVault::open(vault.path(), read_only, data.path()).unwrap()
    }

    #[test]
    fn reads_answer_from_the_files_on_disk() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert_eq!(agent.read("Plan").unwrap()["content"], "The plan about oranges");
        assert_eq!(agent.search("oranges", 10).unwrap()[0]["path"], "Plan.md");
        assert_eq!(agent.backlinks("Plan").unwrap()[0]["sourcePath"], "Home.md");
        assert_eq!(agent.tags(None).unwrap()[0]["tag"], "work");
        assert_eq!(agent.tasks().unwrap()[0]["dueDate"], "2026-11-01");
        assert_eq!(agent.list(None).unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_read_only_handle_refuses_every_write() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        for result in [
            agent.create("New", "x"),
            agent.append("Plan", "x", false),
            agent.rename("Plan", "Plan2", true),
            agent.delete("Plan"),
        ] {
            assert!(result.unwrap_err().contains("read-only"));
        }
        assert_eq!(std::fs::read_to_string(vault.path().join("Plan.md")).unwrap(), "The plan about oranges");
        assert!(!vault.path().join("New.md").exists());
    }

    #[test]
    fn append_keeps_the_old_text_in_version_history() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        agent.append("Plan", "Second line", false).unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.path().join("Plan.md")).unwrap(),
            "The plan about oranges\nSecond line\n"
        );
        let versions = agent.versions("Plan").unwrap();
        let id = versions[0]["id"].as_str().unwrap();
        assert_eq!(agent.version("Plan", id).unwrap()["content"], "The plan about oranges");
    }

    #[test]
    fn append_needs_create_missing_for_a_new_note() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert!(agent.append("Inbox", "idea", false).unwrap_err().contains("does not exist"));
        agent.append("Inbox", "idea", true).unwrap();
        assert_eq!(std::fs::read_to_string(vault.path().join("Inbox.md")).unwrap(), "idea\n");
    }

    #[test]
    fn rename_rewrites_links_and_snapshots_the_notes_it_changes() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        let out = agent.rename("Plan", "Sub/Roadmap", true).unwrap();
        assert_eq!(out["newPath"], "Sub/Roadmap.md");
        assert_eq!(out["notesUpdated"], 1);
        let home = std::fs::read_to_string(vault.path().join("Home.md")).unwrap();
        assert!(home.contains("[[Roadmap]]") || home.contains("[[Sub/Roadmap]]"), "{home}");
        let before = agent.versions("Home").unwrap();
        let id = before[0]["id"].as_str().unwrap();
        assert!(agent.version("Home", id).unwrap()["content"].as_str().unwrap().contains("[[Plan]]"));
    }

    #[test]
    fn paths_cannot_leave_the_vault() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        assert!(agent.read("../outside").is_err());
        assert!(agent.create("../outside", "x").is_err());
    }

    #[test]
    fn history_uses_the_apps_vault_identity_and_follows_renames() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, false);
        agent.append("Plan", "new text", false).unwrap();
        let app_versions = versions::versions_dir(data.path(), vault.path());
        assert_eq!(versions::list(&app_versions, "Plan.md").len(), 1);
        agent.rename("Plan", "Sub/Roadmap", true).unwrap();
        assert_eq!(agent.versions("Sub/Roadmap").unwrap().as_array().unwrap().len(), 1);
        assert!(versions::list(&app_versions, "Plan.md").is_empty());
    }

    #[test]
    fn folder_moves_rewrite_incoming_and_outgoing_links_and_move_history() {
        let (vault, data) = vault_with_notes();
        std::fs::write(vault.path().join("Sub/Inner.md"), "[[Sub/Inner]] [Plan](../Plan.md) ![image](image.png)").unwrap();
        std::fs::write(vault.path().join("Sub/image.png"), "image").unwrap();
        std::fs::write(vault.path().join("Home.md"), "[[Sub/Inner]] [Inner](Sub/Inner.md) ![image](Sub/image.png)").unwrap();
        let agent = open(&vault, &data, false);
        agent.append("Sub/Inner", "more", false).unwrap();
        agent.rename("Sub", "Nested/Moved", true).unwrap();
        let home = agent.read("Home").unwrap()["content"].as_str().unwrap().to_string();
        assert!(home.contains("[[Nested/Moved/Inner]]"), "{home}");
        assert!(home.contains("(Nested/Moved/Inner.md)"), "{home}");
        assert!(home.contains("(Nested/Moved/image.png)"), "{home}");
        let inner = agent.read("Nested/Moved/Inner").unwrap()["content"].as_str().unwrap().to_string();
        assert!(inner.contains("(../../Plan.md)"), "{inner}");
        assert!(inner.contains("(image.png)"), "{inner}");
        assert!(agent.versions("Nested/Moved/Inner").unwrap().as_array().unwrap().len() >= 2);
    }

    #[test]
    fn an_empty_list_folder_means_the_vault_root() {
        let (vault, data) = vault_with_notes();
        let agent = open(&vault, &data, true);
        assert_eq!(agent.list(Some("")).unwrap(), agent.list(None).unwrap());
        assert!(agent.list(Some("../outside")).is_err());
    }

    #[test]
    fn relative_vault_paths_use_the_same_history_as_absolute_paths() {
        let data = tempfile::tempdir().unwrap();
        let relative = std::path::Path::new(".");
        let absolute = std::path::absolute(relative).unwrap();
        let agent = AgentVault::open(relative, true, data.path()).unwrap();
        assert_eq!(agent.versions_dir, versions::versions_dir(data.path(), &absolute));
    }

    #[test]
    #[cfg(unix)]
    fn a_vault_root_alias_keeps_the_apps_history_identity() {
        let (vault, data) = vault_with_notes();
        let alias = data.path().join("Alias");
        std::os::unix::fs::symlink(vault.path(), &alias).unwrap();
        let agent = AgentVault::open(&alias, false, data.path()).unwrap();
        agent.append("Plan", "after", false).unwrap();
        assert_eq!(versions::list(&versions::versions_dir(data.path(), &alias), "Plan.md").len(), 1);
    }

}
