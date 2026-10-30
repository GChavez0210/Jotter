//! Scheduled vault backups: a zip of the whole vault in a folder the user picks.
//!
//! A background loop checks once a minute whether the open vault is due. The backup folder and
//! schedule come from the frontend, per vault. When the last backup ran lives in the app data
//! folder, so restarting Jotter does not start an extra backup.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::fs_atomic::atomic_write_str;

/// Where and how often to back up the open vault.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupConfig {
    /// The vault this schedule belongs to. The loop skips it while another vault is open.
    pub vault: String,
    pub folder: String,
    pub interval_hours: u32,
    pub keep: u32,
}

#[derive(Default)]
pub struct BackupState {
    pub config: Mutex<Option<BackupConfig>>,
    pub last_error: Mutex<Option<String>>,
    /// Held by manual and scheduled backups so they never write at once.
    pub running: std::sync::Arc<Mutex<()>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BackupRecord {
    last_backup_ms: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub last_backup_ms: Option<i64>,
    pub last_error: Option<String>,
}

pub fn record_path(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
    app_data_dir
        .join("vaults")
        .join(format!("{}.backup.json", crate::platform::vault_key(vault_root)))
}

pub fn last_backup_ms(record_file: &Path) -> Option<i64> {
    let raw = std::fs::read_to_string(record_file).ok()?;
    serde_json::from_str::<BackupRecord>(&raw).ok()?.last_backup_ms
}

fn save_last_backup(record_file: &Path, ms: i64) -> Result<(), String> {
    if let Some(parent) = record_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create the Jotter vault settings folder: {e}"))?;
    }
    let json = serde_json::to_string(&BackupRecord { last_backup_ms: Some(ms) })
        .map_err(|e| format!("Cannot serialize the backup record: {e}"))?;
    atomic_write_str(record_file, &json)
}

/// Whether a backup is due: none yet, or the interval has passed since the last one.
pub fn is_due(last_ms: Option<i64>, interval_hours: u32, now_ms: i64) -> bool {
    match last_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= i64::from(interval_hours.max(1)) * 3_600_000,
    }
}

/// Includes seconds and nanoseconds so names sort by backup time.
pub fn backup_file_name(vault_name: &str, now: DateTime<Local>) -> String {
    format!("{}-{}.zip", file_prefix(vault_name), now.format("%Y-%m-%d-%H%M%S-%f"))
}

fn file_prefix(vault_name: &str) -> String {
    let cleaned: String = vault_name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() { "Vault".to_string() } else { trimmed.to_string() }
}

/// Zips `vault_root` into `dest_dir` and returns the new file. The zip is written under a
/// temporary name and renamed, so an interrupted backup never looks like a finished one.
/// Skips symlinks, and `dest_dir` itself when it sits inside the vault.
pub fn create_backup(vault_root: &Path, dest_dir: &Path, vault_name: &str, now: DateTime<Local>) -> Result<PathBuf, String> {
    create_backup_excluding(vault_root, dest_dir, vault_name, now, dest_dir)
}

fn create_backup_excluding(vault_root: &Path, dest_dir: &Path, vault_name: &str, now: DateTime<Local>, excluded: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("Cannot create the backup folder: {e}"))?;
    let skip = Some(excluded.canonicalize().map_err(|e| format!("Cannot resolve the backup folder: {e}"))?);
    if vault_root.canonicalize().map_err(|e| e.to_string())? == *skip.as_ref().unwrap() {
        return Err("Choose a backup folder different from the vault root".into());
    }
    let mut part = tempfile::Builder::new().prefix("").suffix(".part").tempfile_in(dest_dir)
        .map_err(|e| format!("Cannot create a backup temporary file: {e}"))?;
    let suffix = part.path().file_stem().unwrap().to_string_lossy();
    let name = backup_file_name(vault_name, now);
    let target = dest_dir.join(format!("{}-{suffix}.zip", name.trim_end_matches(".zip")));

    let result = (|| -> Result<(), String> {
        let mut zip = zip::ZipWriter::new(part.as_file_mut());
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        add_dir(&mut zip, vault_root, vault_root, skip.as_deref(), options)?;
        let file = zip.finish().map_err(|e| format!("Cannot finish the backup: {e}"))?;
        file.flush().map_err(|e| format!("Cannot write the backup: {e}"))?;
        file.sync_all().map_err(|e| format!("Cannot write the backup: {e}"))
    })();
    result?;
    part.persist_noclobber(&target).map_err(|e| format!("Cannot save {}: {e}", target.display()))?;
    Ok(target)
}

fn add_dir<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    root: &Path,
    dir: &Path,
    skip: Option<&Path>,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        if kind.is_symlink() {
            continue;
        }
        let rel = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let name = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        if kind.is_dir() {
            if skip.is_some_and(|s| path.canonicalize().is_ok_and(|p| p == s)) {
                continue;
            }
            zip.add_directory(format!("{name}/"), options).map_err(|e| format!("Cannot add {name}: {e}"))?;
            add_dir(zip, root, &path, skip, options)?;
        } else if kind.is_file() {
            zip.start_file(&name, options).map_err(|e| format!("Cannot add {name}: {e}"))?;
            let mut src = std::fs::File::open(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            std::io::copy(&mut src, zip).map_err(|e| format!("Cannot add {name}: {e}"))?;
        }
    }
    Ok(())
}

/// Deletes the oldest of this vault's backups until `keep` remain. Touches only files named
/// like Jotter's own backups for this vault.
pub fn prune(dest_dir: &Path, vault_name: &str, keep: u32) -> Result<usize, String> {
    let prefix = format!("{}-", file_prefix(vault_name));
    let mut ours: Vec<String> = std::fs::read_dir(dest_dir)
        .map_err(|e| format!("Cannot read the backup folder: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_backup_name(n, &prefix))
        .collect();
    ours.sort();
    let excess = ours.len().saturating_sub(keep.max(1) as usize);
    for name in &ours[..excess] {
        std::fs::remove_file(dest_dir.join(name)).map_err(|e| format!("Cannot delete the old backup {name}: {e}"))?;
    }
    Ok(excess)
}

/// `<prefix>YYYY-MM-DD-HHMM.zip`, so a vault called "Work" never matches "Work-Notes-…".
fn is_backup_name(name: &str, prefix: &str) -> bool {
    let Some(stamp) = name.strip_prefix(prefix).and_then(|rest| rest.strip_suffix(".zip")) else {
        return false;
    };
    let b = stamp.as_bytes();
    if b.len() == 15 {
        return b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 10 => *c == b'-',
            _ => c.is_ascii_digit(),
        });
    }
    b.len() == 34 && b[27] == b'-'
        && b[..27].iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 10 | 17 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
        && b[28..].iter().all(u8::is_ascii_alphanumeric)
}

/// Runs one backup and its pruning, then records the time. Returns the backup file.
pub fn run(
    vault_root: &Path,
    config: &BackupConfig,
    record_file: &Path,
    now: DateTime<Local>,
) -> Result<PathBuf, String> {
    let name = vault_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Legacy flat backups have no vault identity, so leave them untouched.
    let root = vault_root.canonicalize().map_err(|e| format!("Cannot resolve the vault: {e}"))?;
    let dest = PathBuf::from(config.folder.trim()).join(format!("{}-{}", file_prefix(&name), crate::platform::vault_key(&root)));
    let file = create_backup_excluding(vault_root, &dest, &name, now, Path::new(config.folder.trim()))?;
    save_last_backup(record_file, now.timestamp_millis())?;
    prune(&dest, &name, config.keep)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn moment() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 28, 15, 30, 0).unwrap()
    }

    #[test]
    fn a_backup_is_due_when_none_exists_or_the_interval_passed() {
        let hour = 3_600_000;
        assert!(is_due(None, 24, 0));
        assert!(!is_due(Some(0), 24, 23 * hour));
        assert!(is_due(Some(0), 24, 24 * hour));
        // A clock set back never triggers a backup by itself.
        assert!(!is_due(Some(100 * hour), 24, 0));
    }

    #[test]
    fn file_names_sort_by_age_and_replace_unsafe_characters() {
        assert_eq!(backup_file_name("Work/Notes", moment()), "Work_Notes-2026-10-28-153000-000000000.zip");
        assert_eq!(backup_file_name("..", moment()), "Vault-2026-10-28-153000-000000000.zip");
    }

    #[test]
    fn a_backup_holds_every_note_but_not_symlinks_or_its_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Notes");
        std::fs::create_dir_all(vault.join("Sub")).unwrap();
        std::fs::create_dir_all(vault.join("Backups")).unwrap();
        std::fs::write(vault.join("a.md"), "alpha").unwrap();
        std::fs::write(vault.join("Sub/é.md"), "beta").unwrap();
        std::fs::write(vault.join("Backups/old.zip"), "x").unwrap();

        let file = create_backup(&vault, &vault.join("Backups"), "Notes", moment()).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&file).unwrap()).unwrap();
        let mut names: Vec<String> = archive.file_names().map(|n| n.unwrap().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["Sub/", "Sub/é.md", "a.md"]);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("a.md").unwrap(), &mut text).unwrap();
        assert_eq!(text, "alpha");
        assert!(!vault.join("Backups").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".part")));
    }

    #[test]
    fn pruning_keeps_the_newest_and_leaves_other_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "Work-2026-10-01-0900.zip",
            "Work-2026-10-02-0900.zip",
            "Work-2026-10-03-0900.zip",
            "Work-Notes-2026-10-01-0900.zip",
            "Work-holiday.zip",
            "readme.txt",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        assert_eq!(prune(dir.path(), "Work", 2).unwrap(), 1);
        assert!(!dir.path().join("Work-2026-10-01-0900.zip").exists());
        assert!(dir.path().join("Work-2026-10-02-0900.zip").exists());
        assert!(dir.path().join("Work-Notes-2026-10-01-0900.zip").exists());
        assert!(dir.path().join("Work-holiday.zip").exists());
        assert!(dir.path().join("readme.txt").exists());
    }

    #[test]
    fn running_a_backup_records_the_time() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Notes");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("a.md"), "alpha").unwrap();
        let record = record_path(dir.path(), &vault);
        assert_eq!(last_backup_ms(&record), None);
        let config = BackupConfig { vault: String::new(), folder: dir.path().join("out").to_string_lossy().into_owned(), interval_hours: 24, keep: 3 };
        run(&vault, &config, &record, moment()).unwrap();
        assert_eq!(last_backup_ms(&record), Some(moment().timestamp_millis()));
    }

    #[test]
    fn repeated_backups_keep_distinct_complete_archives() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Notes");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("a.md"), "alpha").unwrap();
        let dest = dir.path().join("Backups");
        let a = create_backup(&vault, &dest, "Notes", moment()).unwrap();
        let b = create_backup(&vault, &dest, "Notes", moment()).unwrap();
        assert_ne!(a, b);
        for file in [&a, &b] {
            let zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
            assert_eq!(zip.len(), 1);
            assert!(is_backup_name(file.file_name().unwrap().to_str().unwrap(), "Notes-"));
        }
        assert_eq!(prune(&dest, "Notes", 1).unwrap(), 1);
    }

    #[test]
    fn same_named_vaults_do_not_overwrite_or_prune_each_others_backups() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in ["a", "b"] {
            let vault = dir.path().join(name).join("Notes");
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::write(vault.join("note.md"), name).unwrap();
            let config = BackupConfig { vault: vault.to_string_lossy().into(), folder: dir.path().join("out").to_string_lossy().into(), interval_hours: 24, keep: 1 };
            let record = record_path(dir.path(), &vault);
            let first = run(&vault, &config, &record, moment()).unwrap();
            let second = run(&vault, &config, &record, moment() + chrono::Duration::minutes(1)).unwrap();
            assert!(!first.exists());
            paths.push(second);
        }
        assert_ne!(paths[0].parent(), paths[1].parent());
        assert!(paths.iter().all(|p| p.exists()));
    }

    #[test]
    fn a_nested_destination_excludes_the_whole_selected_backup_folder() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Notes");
        let folder = vault.join("Backups");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(vault.join("a.md"), "alpha").unwrap();
        std::fs::write(folder.join("legacy.zip"), "old").unwrap();
        let config = BackupConfig { vault: vault.to_string_lossy().into(), folder: folder.to_string_lossy().into(), interval_hours: 24, keep: 1 };
        let file = run(&vault, &config, &record_path(dir.path(), &vault), moment()).unwrap();
        let zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
        assert_eq!(zip.len(), 1);
        assert!(create_backup(&vault, &vault, "Notes", moment()).is_err());
    }

    #[test]
    fn concurrent_backups_never_share_a_temporary_or_final_file() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("Notes");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("a.md"), "alpha".repeat(100_000)).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs: Vec<_> = (0..2).map(|_| {
            let vault = vault.clone();
            let dest = dir.path().join("Backups");
            let barrier = barrier.clone();
            std::thread::spawn(move || { barrier.wait(); create_backup(&vault, &dest, "Notes", moment()).unwrap() })
        }).collect();
        let files: Vec<_> = jobs.into_iter().map(|j| j.join().unwrap()).collect();
        assert_ne!(files[0], files[1]);
        for file in files {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(file).unwrap()).unwrap();
            let mut text = String::new();
            std::io::Read::read_to_string(&mut zip.by_name("a.md").unwrap(), &mut text).unwrap();
            assert_eq!(text, "alpha".repeat(100_000));
        }
    }

}
