//! Note version history: snapshots of each note, kept per vault in the app data folder.
//!
//! Snapshots are a safety net, not vault content, so they live beside the index cache and the
//! pins file rather than inside the vault, where a sync client would carry one computer's
//! history to another (decision 3). Each note gets a folder named by a hash of its path, so
//! a note in a deep folder, or one with characters the filesystem dislikes, still fits.
//!
//! A save stores the text the file held *before* it, so the history is a trail of states the
//! note can go back to. Saves come from autosave every few hundred milliseconds, so a snapshot
//! is taken only when the newest one is older than [`SNAPSHOT_INTERVAL_MS`], and older
//! snapshots are thinned by age on every write.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs_atomic::atomic_write_str;

/// Shortest gap between two snapshots of one note. Typing continuously fills the history at
/// this rate; the state before a session of edits is kept by the first save after it opens.
pub const SNAPSHOT_INTERVAL_MS: i64 = 5 * 60 * 1000;

/// Snapshots kept per note, whatever their age. Thinning usually bites first.
const MAX_VERSIONS_PER_NOTE: usize = 60;

/// A note larger than this is not worth copying on every save.
const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// How long a deleted note's history waits for the note to come back from the trash.
const ORPHAN_GRACE_MS: i64 = 30 * DAY_MS;

/// One stored version, newest first in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteVersion {
    /// The snapshot's file stem, which is the time it was taken in milliseconds.
    pub id: String,
    pub saved_at_ms: i64,
    pub bytes: u64,
}

/// On-disk marker naming the note a folder belongs to. The folder name is only a hash.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteMarker {
    note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    missing_since_ms: Option<i64>,
}

/// Validate before any history operation can read note bytes or derive a history key.
pub fn resolve_note_path(vault_root: &Path, rel_path: &str) -> Result<(PathBuf, String), String> {
    crate::vault::resolve_safe_vault_path(vault_root, &crate::vault::ensure_md_extension(rel_path))
}

/// A restore must preserve the current text, including when history storage is unavailable.
pub fn snapshot_before_restore(dir: &Path, rel_path: &str, content: &str, now_ms: i64) -> Result<(), String> {
    if content.len() > MAX_SNAPSHOT_BYTES {
        return Err("Cannot restore: the current note is too large to preserve in version history.".into());
    }
    snapshot(dir, rel_path, content, now_ms, 0)
        .map_err(|e| format!("Cannot preserve the current note before restoring: {e}"))?;
    Ok(())
}

fn write_marker(dir: &Path, note: &str, missing_since_ms: Option<i64>) -> Result<(), String> {
    let marker = serde_json::to_string(&NoteMarker { note: note.to_string(), missing_since_ms })
        .map_err(|e| format!("Cannot record the note path: {e}"))?;
    atomic_write_str(&dir.join("note.json"), &marker)
}

/// The versions folder for one vault, named by the same key as its index database.
pub fn versions_dir(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
    app_data_dir
        .join("vaults")
        .join(format!("{}.versions", crate::platform::vault_key(vault_root)))
}

/// The folder holding one note's snapshots.
pub fn note_dir(versions_dir: &Path, rel_path: &str) -> PathBuf {
    versions_dir.join(crate::platform::note_key(rel_path))
}

/// Stores `content` as the note's state at `now_ms`, unless a snapshot is already that recent
/// or holds the same text. Returns whether a snapshot was written.
///
/// `min_interval_ms` is [`SNAPSHOT_INTERVAL_MS`] for ordinary saves; restoring a version passes
/// 0, so the text being replaced is always kept.
pub fn snapshot(
    versions_dir: &Path,
    rel_path: &str,
    content: &str,
    now_ms: i64,
    min_interval_ms: i64,
) -> Result<bool, String> {
    if content.len() > MAX_SNAPSHOT_BYTES {
        return Ok(false);
    }
    let dir = note_dir(versions_dir, rel_path);
    let stored = read_versions(&dir);

    if let Some(newest) = stored.first() {
        if min_interval_ms > 0 && now_ms - newest.saved_at_ms < min_interval_ms {
            return Ok(false);
        }
        let newest_text = std::fs::read_to_string(dir.join(format!("{}.md", newest.id)));
        if newest_text.is_ok_and(|text| text == content) {
            return Ok(false);
        }
    }

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create the version history folder: {e}"))?;
    let id = match stored.first() {
        Some(newest) if newest.saved_at_ms >= now_ms => newest.saved_at_ms.checked_add(1).ok_or("Version timestamp overflow")?,
        _ => now_ms,
    };
    atomic_write_str(&dir.join(format!("{id}.md")), content)?;
    write_marker(&dir, rel_path, None)?;
    prune(&dir, now_ms);
    Ok(true)
}

/// Every stored version of a note, newest first.
pub fn list(versions_dir: &Path, rel_path: &str) -> Vec<NoteVersion> {
    read_versions(&note_dir(versions_dir, rel_path))
}

/// The text of one version.
pub fn read(versions_dir: &Path, rel_path: &str, id: &str) -> Result<String, String> {
    if !is_version_id(id) {
        return Err(format!("'{id}' is not a version"));
    }
    let file = note_dir(versions_dir, rel_path).join(format!("{id}.md"));
    std::fs::read_to_string(&file).map_err(|e| format!("Cannot read that version: {e}"))
}

/// Moves a note's history when the note is renamed or moved, including every note inside a
/// renamed folder. Folder names are hashes, so each one is read back from its marker.
pub fn rename(versions_dir: &Path, old_rel: &str, new_rel: &str) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(versions_dir) else {
        return Ok(());
    };
    let moves: Vec<(PathBuf, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let dir = entry.path();
            let note = read_marker(&dir)?;
            renamed_path(&note, old_rel, new_rel).map(|next| (dir, next))
        })
        .collect();

    for (dir, next) in moves {
        let target = note_dir(versions_dir, &next);
        if target == dir {
            write_marker(&target, &next, None)?;
            continue;
        }
        if target.exists() {
            // Timestamp collisions can hold different text. Preserve both before removing
            // the source, and leave it intact if any copy fails.
            for version in read_versions(&dir) {
                let text = std::fs::read_to_string(dir.join(format!("{}.md", version.id)))
                    .map_err(|e| format!("Cannot read history while merging: {e}"))?;
                let mut id = version.saved_at_ms;
                loop {
                    let file = target.join(format!("{id}.md"));
                    match std::fs::read_to_string(&file) {
                        Ok(existing) if existing == text => break,
                        Ok(_) => id = id.checked_add(1).ok_or("Version timestamp overflow")?,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            atomic_write_str(&file, &text)?;
                            break;
                        }
                        Err(e) => return Err(format!("Cannot merge version history: {e}")),
                    }
                }
            }
            write_marker(&target, &next, None)?;
            std::fs::remove_dir_all(&dir).map_err(|e| format!("Cannot finish moving version history: {e}"))?;
            continue;
        }
        std::fs::rename(&dir, &target)
            .map_err(|e| format!("Cannot move a note's version history: {e}"))?;
        write_marker(&target, &next, None)?;
    }
    Ok(())
}

/// Drops the history of notes that are no longer in the vault and have not been for a month,
/// so a note restored from the trash still finds its versions. Runs when a vault opens.
pub fn prune_orphans(versions_dir: &Path, vault_root: &Path, now_ms: i64) {
    let Ok(entries) = std::fs::read_dir(versions_dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        let Some(marker) = std::fs::read_to_string(dir.join("note.json")).ok()
            .and_then(|raw| serde_json::from_str::<NoteMarker>(&raw).ok()) else { continue };
        if vault_root.join(&marker.note).is_file() {
            if marker.missing_since_ms.is_some() { let _ = write_marker(&dir, &marker.note, None); }
            continue;
        }
        let Some(missing_since) = marker.missing_since_ms else {
            let _ = write_marker(&dir, &marker.note, Some(now_ms));
            continue;
        };
        if now_ms - missing_since > ORPHAN_GRACE_MS {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// Thins the snapshots in one note's folder and applies the per-note cap.
fn prune(note_dir: &Path, now_ms: i64) {
    let stored = read_versions(note_dir);
    for id in dropped_versions(&stored, now_ms) {
        let _ = std::fs::remove_file(note_dir.join(format!("{id}.md")));
    }
}

/// Picks the versions to drop: everything from the last day is kept, then one per day for a
/// month, one per week for a year, and nothing older. Oldest go first once the cap is reached.
///
/// `stored` is newest first, as [`read_versions`] returns it.
pub fn dropped_versions(stored: &[NoteVersion], now_ms: i64) -> Vec<String> {
    let mut dropped = Vec::new();
    let mut kept = 0usize;
    let mut last_bucket: Option<(u8, i64)> = None;

    for version in stored {
        let age = now_ms - version.saved_at_ms;
        let bucket = if age < DAY_MS {
            // Everything from the last day stands on its own.
            Some((0, version.saved_at_ms))
        } else if age < 30 * DAY_MS {
            Some((1, version.saved_at_ms / DAY_MS))
        } else if age < 365 * DAY_MS {
            Some((2, version.saved_at_ms / (7 * DAY_MS)))
        } else {
            None
        };

        let keep = match bucket {
            None => false,
            Some(b) => last_bucket != Some(b) && kept < MAX_VERSIONS_PER_NOTE,
        };
        if keep {
            kept += 1;
            last_bucket = bucket;
        } else {
            dropped.push(version.id.clone());
        }
    }
    dropped
}

/// Reads a note's folder, newest first. Files that are not snapshots are ignored.
fn read_versions(note_dir: &Path) -> Vec<NoteVersion> {
    let Ok(entries) = std::fs::read_dir(note_dir) else {
        return Vec::new();
    };
    let mut versions: Vec<NoteVersion> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            if !is_version_id(&id) {
                return None;
            }
            Some(NoteVersion {
                saved_at_ms: id.parse().ok()?,
                bytes: entry.metadata().ok()?.len(),
                id,
            })
        })
        .collect();
    versions.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
    versions
}

fn read_marker(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("note.json")).ok()?;
    serde_json::from_str::<NoteMarker>(&raw).ok().map(|m| m.note)
}

/// The note's path after a rename of `old_rel`, or `None` when the rename does not touch it.
fn renamed_path(note: &str, old_rel: &str, new_rel: &str) -> Option<String> {
    if paths_equal(note, old_rel) {
        return Some(new_rel.to_string());
    }
    let prefix_len = old_rel.len() + 1;
    let head = note.get(..prefix_len)?;
    let rest = note.get(prefix_len..)?;
    if !rest.is_empty() && paths_equal(head, &format!("{old_rel}/")) {
        Some(format!("{new_rel}/{rest}"))
    } else {
        None
    }
}

/// Windows and macOS open `Note.md` and `note.md` as one file, as the vault does.
fn paths_equal(a: &str, b: &str) -> bool {
    if cfg!(target_os = "linux") {
        a == b
    } else {
        a.eq_ignore_ascii_case(b)
    }
}

/// A snapshot is named by its timestamp, so nothing else in the folder can be read as one.
fn is_version_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 19 && id.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: i64) -> NoteVersion {
        NoteVersion { id: id.to_string(), saved_at_ms: id, bytes: 1 }
    }

    #[test]
    fn a_save_keeps_the_text_the_file_held_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("vault.versions");
        let now = 1_700_000_000_000;

        assert!(snapshot(&versions, "Work/Note.md", "first", now, SNAPSHOT_INTERVAL_MS).unwrap());
        // A second save moments later adds nothing: autosave fires every few hundred ms.
        assert!(!snapshot(&versions, "Work/Note.md", "second", now + 1000, SNAPSHOT_INTERVAL_MS).unwrap());
        // Past the interval it does.
        let later = now + SNAPSHOT_INTERVAL_MS + 1;
        assert!(snapshot(&versions, "Work/Note.md", "second", later, SNAPSHOT_INTERVAL_MS).unwrap());
        // Text that has not changed is never stored twice.
        let later2 = later + SNAPSHOT_INTERVAL_MS + 1;
        assert!(!snapshot(&versions, "Work/Note.md", "second", later2, SNAPSHOT_INTERVAL_MS).unwrap());
        // Restoring passes no interval, so the text it replaces is always kept.
        assert!(snapshot(&versions, "Work/Note.md", "third", later + 10, 0).unwrap());

        let listed = list(&versions, "Work/Note.md");
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].saved_at_ms, later + 10);
        assert_eq!(read(&versions, "Work/Note.md", &listed[0].id).unwrap(), "third");
        assert_eq!(read(&versions, "Work/Note.md", &listed[2].id).unwrap(), "first");
        // Another note keeps its own history.
        assert!(list(&versions, "Other.md").is_empty());
    }

    #[test]
    fn reading_refuses_anything_but_a_version_id() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("vault.versions");
        snapshot(&versions, "A.md", "text", 1000, 0).unwrap();
        assert!(read(&versions, "A.md", "../../note").is_err());
        assert!(read(&versions, "A.md", "note").is_err());
        assert!(read(&versions, "A.md", "").is_err());
        // The marker file is not a version.
        assert_eq!(list(&versions, "A.md").len(), 1);
    }

    #[test]
    fn thinning_keeps_a_day_of_detail_then_days_then_weeks() {
        let now = 400 * DAY_MS;
        let stored = vec![
            version(now - HOUR_MS),                  // today: kept
            version(now - 2 * HOUR_MS),              // today: kept
            version(now - 3 * HOUR_MS),              // today: kept
            version(now - 2 * DAY_MS - HOUR_MS),     // same day as the next one
            version(now - 2 * DAY_MS - 2 * HOUR_MS), // dropped: one per day
            version(now - 40 * DAY_MS),              // same week as the next one
            version(now - 41 * DAY_MS),              // dropped: one per week
            version(now - 370 * DAY_MS),             // dropped: older than a year
        ];
        let dropped = dropped_versions(&stored, now);
        assert_eq!(
            dropped,
            vec![
                (now - 2 * DAY_MS - 2 * HOUR_MS).to_string(),
                (now - 41 * DAY_MS).to_string(),
                (now - 370 * DAY_MS).to_string(),
            ]
        );
        // Nothing from the last day is ever thinned.
        let busy: Vec<NoteVersion> = (0..50).map(|i| version(now - i * 60_000)).collect();
        assert!(dropped_versions(&busy, now).is_empty());
    }

    #[test]
    fn the_cap_drops_the_oldest_versions_first() {
        let now = 100 * DAY_MS;
        // A minute apart, so every one of them is inside the day that is never thinned.
        let stored: Vec<NoteVersion> = (0..MAX_VERSIONS_PER_NOTE as i64 + 20)
            .map(|i| version(now - i * 60_000))
            .collect();
        let dropped = dropped_versions(&stored, now);
        assert_eq!(stored.len() - dropped.len(), MAX_VERSIONS_PER_NOTE);
        // The ones that go are the oldest.
        assert!(dropped.contains(&stored[stored.len() - 1].id));
        assert!(!dropped.contains(&stored[0].id));
    }

    #[test]
    fn thinning_runs_on_every_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("vault.versions");
        let start = 500 * DAY_MS;
        for i in 0..6 {
            // Six saves within one day, so none of them is thinned yet.
            snapshot(&versions, "A.md", &format!("v{i}"), start + i * HOUR_MS, 0).unwrap();
        }
        assert_eq!(list(&versions, "A.md").len(), 6);
        // A save a year later leaves only the newest, which that save adds.
        snapshot(&versions, "A.md", "new", start + 400 * DAY_MS, 0).unwrap();
        let listed = list(&versions, "A.md");
        assert_eq!(listed.len(), 1);
        assert_eq!(read(&versions, "A.md", &listed[0].id).unwrap(), "new");
    }

    #[test]
    fn history_follows_a_renamed_note_and_a_renamed_folder() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("vault.versions");
        snapshot(&versions, "Work/Report.md", "report", 1000, 0).unwrap();
        snapshot(&versions, "Work/Deep/Notes.md", "notes", 1000, 0).unwrap();
        snapshot(&versions, "Workshop/Other.md", "other", 1000, 0).unwrap();

        rename(&versions, "Work/Report.md", "Work/Summary.md").unwrap();
        assert!(list(&versions, "Work/Report.md").is_empty());
        assert_eq!(list(&versions, "Work/Summary.md").len(), 1);

        rename(&versions, "Work", "Archive").unwrap();
        assert_eq!(list(&versions, "Archive/Summary.md").len(), 1);
        assert_eq!(list(&versions, "Archive/Deep/Notes.md").len(), 1);
        // A folder with a similar name is left alone.
        assert_eq!(list(&versions, "Workshop/Other.md").len(), 1);

        // The renamed note keeps its text, so the history is still readable.
        let listed = list(&versions, "Archive/Summary.md");
        assert_eq!(read(&versions, "Archive/Summary.md", &listed[0].id).unwrap(), "report");
    }

    #[test]
    fn a_deleted_note_keeps_its_history_for_a_month() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let versions = dir.path().join("vault.versions");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("Kept.md"), "kept").unwrap();

        let now = 1_000 * DAY_MS;
        snapshot(&versions, "Kept.md", "kept", now, 0).unwrap();
        snapshot(&versions, "Gone.md", "gone", now, 0).unwrap();

        // A note in the trash still has its history waiting.
        prune_orphans(&versions, &vault, now + DAY_MS);
        assert_eq!(list(&versions, "Gone.md").len(), 1);

        prune_orphans(&versions, &vault, now + DAY_MS + ORPHAN_GRACE_MS + 1);
        assert!(list(&versions, "Gone.md").is_empty());
        assert_eq!(list(&versions, "Kept.md").len(), 1);
    }

    #[test]
    fn each_vault_and_each_note_keeps_its_own_folder() {
        let data = Path::new("C:/data");
        assert_ne!(
            versions_dir(data, Path::new("C:/Notes/Work")),
            versions_dir(data, Path::new("C:/Notes/Personal"))
        );
        let versions = versions_dir(data, Path::new("C:/Notes/Work"));
        assert_ne!(note_dir(&versions, "A.md"), note_dir(&versions, "B.md"));
        // One note reached through two spellings has one history, except on Linux.
        if cfg!(not(target_os = "linux")) {
            assert_eq!(note_dir(&versions, "A.md"), note_dir(&versions, "a.MD"));
        }
    }

    #[test]
    fn a_note_too_large_to_copy_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let versions = dir.path().join("vault.versions");
        let huge = "x".repeat(MAX_SNAPSHOT_BYTES + 1);
        assert!(!snapshot(&versions, "Big.md", &huge, 1000, 0).unwrap());
        assert!(list(&versions, "Big.md").is_empty());
    }

    #[test]
    fn history_paths_reject_traversal_and_hidden_files_before_reading() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(temp.path().join("outside.md"), "private fixture").unwrap();
        for path in ["../outside.md", "folder/../../outside.md", ".obsidian/secret.md", "C:/outside.md"] {
            assert!(resolve_note_path(&vault, path).is_err(), "{path}");
        }
        assert_eq!(resolve_note_path(&vault, "Folder/Note").unwrap().1, "Folder/Note.md");
    }

    #[test]
    fn history_merge_preserves_both_texts_when_timestamps_collide() {
        let temp = tempfile::tempdir().unwrap();
        snapshot(temp.path(), "A.md", "A history", 1000, 0).unwrap();
        snapshot(temp.path(), "B.md", "B history", 1000, 0).unwrap();
        rename(temp.path(), "A.md", "B.md").unwrap();
        let mut texts: Vec<_> = list(temp.path(), "B.md").iter()
            .map(|v| read(temp.path(), "B.md", &v.id).unwrap()).collect();
        texts.sort();
        assert_eq!(texts, vec!["A history", "B history"]);
        assert!(list(temp.path(), "A.md").is_empty());
        assert_eq!(read_marker(&note_dir(temp.path(), "B.md")).unwrap(), "B.md");
    }

    #[test]
    fn old_snapshots_get_a_full_grace_period_after_disappearance() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let versions = temp.path().join("versions");
        snapshot(&versions, "Old.md", "old history", 1000, 0).unwrap();
        let now = 400 * DAY_MS;
        prune_orphans(&versions, &vault, now);
        assert_eq!(list(&versions, "Old.md").len(), 1);
        prune_orphans(&versions, &vault, now + 29 * DAY_MS);
        assert_eq!(list(&versions, "Old.md").len(), 1);
        std::fs::write(vault.join("Old.md"), "back").unwrap();
        prune_orphans(&versions, &vault, now + 30 * DAY_MS);
        std::fs::remove_file(vault.join("Old.md")).unwrap();
        prune_orphans(&versions, &vault, now + 31 * DAY_MS);
        assert_eq!(list(&versions, "Old.md").len(), 1);
        prune_orphans(&versions, &vault, now + 62 * DAY_MS);
        assert!(list(&versions, "Old.md").is_empty());
    }

    #[test]
    fn restore_refuses_unavailable_history_and_oversized_backups() {
        let temp = tempfile::tempdir().unwrap();
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        assert!(snapshot_before_restore(&blocked, "A.md", "unsaved text", 1000).is_err());
        assert!(snapshot_before_restore(temp.path(), "A.md", &"x".repeat(MAX_SNAPSHOT_BYTES + 1), 1000).is_err());
    }

    #[test]
    fn restores_in_one_millisecond_keep_each_state() {
        let temp = tempfile::tempdir().unwrap();
        snapshot_before_restore(temp.path(), "A.md", "one", 1000).unwrap();
        snapshot_before_restore(temp.path(), "A.md", "two", 1000).unwrap();
        snapshot_before_restore(temp.path(), "A.md", "three", 1000).unwrap();
        assert_eq!(list(temp.path(), "A.md").len(), 3);
        assert_eq!(read(temp.path(), "A.md", "1000").unwrap(), "one");
        assert_eq!(read(temp.path(), "A.md", "1002").unwrap(), "three");
    }
}
