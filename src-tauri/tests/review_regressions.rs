use jotter_lib::{index::*, media_export, vault};
use rusqlite::Connection;
use std::fs;

fn note(path: &str, modified: i64) -> ScannedNoteFile {
    ScannedNoteFile {
        path: path.into(), title: path.into(), fs_modified_ms: modified, created_ms: 0,
        preview: String::new(), content: String::new(), links: vec![], tags: vec!["test".into()],
    }
}

#[test]
fn unicode_note_names_create_and_rename_without_panicking() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["日a", "aé", "📝a", "別a.MD"] {
        let (_, doc) = vault::create_vault_note(temp.path(), name, "content").unwrap();
        assert!(doc.rel_path.to_lowercase().ends_with(".md"));
    }
    vault::rename_vault_entry(temp.path(), "aé.md", "éa").unwrap();
    assert_eq!(vault::read_vault_note(temp.path(), "éa.md").unwrap().content, "content");
}

#[test]
fn wildcard_folder_deletion_and_history_rename_are_literal() {
    let conn = Connection::open_in_memory().unwrap();
    init_index_schema(&conn).unwrap();
    for folder in ["A_B", "AxB", "A%B", "AxxB"] {
        let file = format!("{folder}/note.md");
        upsert_scanned_note(&conn, &note(&file, 2000), Some(1000)).unwrap();
    }
    rename_edit_log_paths(&conn, "A_B", "renamed").unwrap();
    assert_eq!(latest_edit_log_ms(&conn, "AxB/note.md").unwrap(), Some(1000));
    assert_eq!(latest_edit_log_ms(&conn, "renamed/note.md").unwrap(), Some(1000));
    delete_indexed_path(&conn, "A_B").unwrap();
    delete_indexed_path(&conn, "A%B").unwrap();
    let paths: Vec<String> = conn.prepare("SELECT path FROM notes ORDER BY path").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(paths, ["AxB/note.md", "AxxB/note.md"]);
    let tags: i64 = conn.query_row("SELECT COUNT(*) FROM note_tags", [], |r| r.get(0)).unwrap();
    assert_eq!(tags, 2);
}

#[test]
fn external_changes_use_newer_mtime_and_preserve_edit_history() {
    let conn = Connection::open_in_memory().unwrap();
    init_index_schema(&conn).unwrap();
    upsert_scanned_note(&conn, &note("a.md", 1000), Some(1000)).unwrap();
    upsert_scanned_note(&conn, &note("a.md", 2000), None).unwrap();
    let modified: i64 = conn.query_row("SELECT modified_ms FROM notes", [], |r| r.get(0)).unwrap();
    assert_eq!(modified, 2000);
    assert_eq!(latest_edit_log_ms(&conn, "a.md").unwrap(), Some(1000));
}

#[test]
fn failed_index_batches_roll_back_instead_of_leaving_partial_rows() {
    let conn = Connection::open_in_memory().unwrap();
    init_index_schema(&conn).unwrap();
    upsert_scanned_note(&conn, &note("old.md", 1000), None).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_bad BEFORE INSERT ON notes WHEN NEW.path='bad.md' BEGIN SELECT RAISE(ABORT,'failed'); END;").unwrap();
    assert!(apply_incremental_index(&conn, &["old.md".into()], &[note("good.md", 1000), note("bad.md", 1000)], None).is_err());
    let paths: Vec<String> = conn.prepare("SELECT path FROM notes").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(paths, ["old.md"]);
}

#[cfg(windows)]
#[test]
fn junctions_cannot_escape_vault_or_create_index_cycles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("safe.md"), "safe").unwrap();
    fs::write(outside.join("secret.md"), "secret").unwrap();
    let link = root.join("attachments");
    let command = format!("New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null", link.display().to_string().replace('\'', "''"), outside.display().to_string().replace('\'', "''"));
    assert!(std::process::Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &command]).status().unwrap().success());
    assert!(vault::read_vault_note(&root, "attachments/secret.md").is_err());
    assert!(vault::save_vault_note(&root, "attachments/secret.md", "changed").is_err());
    assert!(vault::create_vault_note(&root, "attachments/new", "new").is_err());
    assert!(media_export::read_vault_image_as_data_url(&root, "attachments/secret.md").is_err());
    assert!(media_export::save_png_to_vault_attachments(&root, b"png", None).is_err());
    assert_eq!(vault::scan_vault_snapshot(&root).unwrap().note_count, 1);
    assert_eq!(scan_dir_files_from_disk(&root, &root).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(outside.join("secret.md")).unwrap(), "secret");
    fs::remove_dir(&link).unwrap();
    let command = format!("New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null", link.display(), root.display());
    assert!(std::process::Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &command]).status().unwrap().success());
    assert_eq!(scan_dir_files_from_disk(&root, &root).unwrap().len(), 1);
    fs::remove_dir(&link).unwrap();
}


#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_vault_or_create_index_cycles() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("vault");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("safe.md"), "safe").unwrap();
    fs::write(outside.join("secret.md"), "secret").unwrap();
    let link = root.join("attachments");
    symlink(&outside, &link).unwrap();
    assert!(vault::read_vault_note(&root, "attachments/secret.md").is_err());
    assert!(vault::save_vault_note(&root, "attachments/secret.md", "changed").is_err());
    assert!(vault::create_vault_note(&root, "attachments/new", "new").is_err());
    assert!(media_export::read_vault_image_as_data_url(&root, "attachments/secret.md").is_err());
    assert!(media_export::save_png_to_vault_attachments(&root, b"png", None).is_err());
    assert_eq!(vault::scan_vault_snapshot(&root).unwrap().note_count, 1);
    assert_eq!(scan_dir_files_from_disk(&root, &root).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(outside.join("secret.md")).unwrap(), "secret");
    fs::remove_file(&link).unwrap();
    symlink(&root, &link).unwrap();
    assert_eq!(scan_dir_files_from_disk(&root, &root).unwrap().len(), 1);
    fs::remove_file(&link).unwrap();
}
