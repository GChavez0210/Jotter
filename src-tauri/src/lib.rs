pub mod ai;
pub mod app_settings;
pub mod backup;
pub mod caldav;
pub mod capture;
pub mod clipboard;
pub use jotter_core::fs_atomic;
pub use jotter_core::health;
pub use jotter_core::index;
pub use jotter_core::link_update;
pub use jotter_core::links;
pub use jotter_core::md_scan;
pub mod media_export;
pub mod pins;
pub use jotter_core::platform;
pub use jotter_core::recycle;
pub mod reminders;
pub use jotter_core::search;
pub use jotter_core::semantic;
pub mod snippets;
pub mod spell;
pub use jotter_core::tasks;
pub mod tray;
pub use jotter_core::types;
pub mod updates;
pub use jotter_core::vault;
pub use jotter_core::vector_store;
pub use jotter_core::versions;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

use crate::clipboard::{
    build_clipboard_payload, read_plain_from_system_clipboard, write_to_system_clipboard,
};
use crate::index::{
    apply_incremental_index, apply_scanned_vault_index, delete_indexed_path, init_index_schema,
    query_all_tags, query_backlinks_for_note, query_calendar_month, query_notes_by_tag,
    query_notes_on_date, rename_edit_log_paths, scan_dir_files_from_disk,
    scan_single_file_from_disk, system_time_to_ms, upsert_scanned_note, ScannedNoteFile,
};
use crate::search::VaultSearchIndex;
use crate::types::{
    BacklinkEntry, CalendarDateField, CalendarDaySummary, CalendarNoteEntry,
    ClipboardPayload, CopyRequest, CssSnippet, DeletedEntry, NoteDocument, RecentVault,
    NoteCard, RenameEntryResult, SearchHit, SpellDictionaryInfo, TagSummary, TaskEntry, VaultFsEvent, VaultSnapshot,
};
use crate::vault::{
    create_vault_folder, create_vault_note, delete_vault_entry, load_recent_vaults,
    read_vault_note, record_recent_vault, remove_recent_vault, rename_vault_entry, save_vault_note,
    scan_vault_snapshot, validate_entry_name,
};

pub struct AppState {
    pub active_vault: Mutex<Option<PathBuf>>,
    pub sqlite_conn: Arc<Mutex<Connection>>,
    pub search_index: Mutex<Arc<VaultSearchIndex>>,
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    pub recent_self_writes: SelfWrites,
    pub recent_vaults_file: PathBuf,
    /// Bumped on every vault open or close so a stale watcher worker never writes into the next vault's index.
    pub vault_generation: Arc<AtomicU64>,
    next_vault_generation: AtomicU64,
    index_dirty: Arc<AtomicBool>,
    pub vault_operation: Arc<Mutex<()>>,
    /// Whether a save keeps the note's previous text. The frontend owns the setting.
    pub version_history: Arc<AtomicBool>,
}

/// Path of the SQLite index cache for `vault_root`: one database per vault, so edit history
/// and index rows from one vault never leak into another vault with the same relative paths.
fn vault_index_db_path(app_data_dir: &Path, vault_root: &Path) -> PathBuf {
    app_data_dir
        .join("indexes")
        .join(format!("{}.sqlite", crate::platform::vault_key(vault_root)))
}

fn require_active_vault(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let guard = state
        .active_vault
        .lock_recover();
    guard
        .clone()
        .ok_or_else(|| "No vault is currently open".to_string())
}

/// A note write the app made itself, remembered briefly so the file watcher does not report it back.
pub struct SelfWrite {
    at: Instant,
    /// A hash of the text written. With it, a change by another program that lands right after
    /// the app's write is still reported, because the file no longer matches what the app wrote.
    hash: Option<u64>,
}

pub type SelfWrites = Arc<Mutex<HashMap<String, SelfWrite>>>;

/// Mutex access that carries on after another thread panicked while holding the lock. The data
/// is the index cache and small flags, which a rebuild or the next write repairs, so one panic
/// must not make every later command fail.
trait LockRecover<T> {
    fn lock_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockRecover<T> for Mutex<T> {
    fn lock_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn hash_text(text: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn mark_self_write(recent_self_writes: &SelfWrites, rel_path: &str) {
    let mut map = recent_self_writes.lock_recover();
    let now = Instant::now();
    map.retain(|_, write| now.duration_since(write.at) < Duration::from_secs(3));
    map.insert(rel_path.to_lowercase(), SelfWrite { at: now, hash: None });
}

/// Records the text the app just wrote to `rel_path`, so only a file that still holds it counts as the app's own.
fn record_self_write_content(recent_self_writes: &SelfWrites, rel_path: &str, content: &str) {
    let mut map = recent_self_writes.lock_recover();
    map.insert(
        rel_path.to_lowercase(),
        SelfWrite { at: Instant::now(), hash: Some(hash_text(content.as_bytes())) },
    );
}

fn is_recent_self_write(recent_self_writes: &SelfWrites, vault_root: &Path, rel_path: &str) -> bool {
    let (recent, hash) = {
        let mut map = recent_self_writes.lock_recover();
        let now = Instant::now();
        map.retain(|_, write| now.duration_since(write.at) < Duration::from_secs(3));
        match map.get(&rel_path.to_lowercase()) {
            Some(write) => (now.duration_since(write.at) < Duration::from_millis(900), write.hash),
            None => return false,
        }
    };
    if !recent {
        return false;
    }
    let Some(hash) = hash else { return true };
    let Ok((path, _)) = crate::vault::resolve_safe_vault_path(vault_root, rel_path) else {
        return false;
    };
    std::fs::read(path).is_ok_and(|bytes| hash_text(&bytes) == hash)
}

/// Processes a debounced batch of changed relative paths incrementally:
/// walks/reads any changed files or folders outside the SQLite/Tantivy locks, then updates
/// SQLite (`notes`, `links`, `note_tags`) and Tantivy incrementally.
fn process_watcher_batch(
    vault_root: &Path,
    sqlite_conn: &Arc<Mutex<Connection>>,
    search_index: &Arc<VaultSearchIndex>,
    index_dirty: &AtomicBool,
    batch_rel_paths: &[String],
) {
    enum IncrementalOp {
        Delete(String),
        SingleFile(ScannedNoteFile),
        DirFiles(String, Vec<ScannedNoteFile>),
    }

    // 1. Perform all filesystem checks, file reads, and directory walks WITHOUT holding locks
    let mut ops = Vec::with_capacity(batch_rel_paths.len());
    for rel_path in batch_rel_paths {
        let abs_path = vault_root.join(rel_path);
        if !abs_path.exists() {
            ops.push(IncrementalOp::Delete(rel_path.clone()));
        } else if abs_path.is_file() {
            if abs_path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                if let Ok(scanned) = scan_single_file_from_disk(vault_root, &abs_path) {
                    ops.push(IncrementalOp::SingleFile(scanned));
                }
            }
        } else if abs_path.is_dir() {
            if let Ok(scanned_dir_files) = scan_dir_files_from_disk(vault_root, &abs_path) {
                ops.push(IncrementalOp::DirFiles(rel_path.clone(), scanned_dir_files));
            }
        }
    }

    let mut deletions = Vec::new();
    let mut files = Vec::new();
    for op in ops {
        match op {
            IncrementalOp::Delete(path) => deletions.push(path),
            IncrementalOp::SingleFile(file) => files.push(file),
            IncrementalOp::DirFiles(path, scanned) => {
                deletions.push(path);
                files.extend(scanned);
            }
        }
    }
    let conn = sqlite_conn.lock_recover();
    if let Err(err) = apply_incremental_index(&conn, &deletions, &files, None) {
        index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Index update failed: {err}");
    }
    drop(conn);
    if let Err(err) = search_index.apply_changes(&deletions, &files) {
        index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Search update failed: {err}");
    }
}

fn start_vault_watcher(
    app_handle: AppHandle,
    vault_root: PathBuf,
    state: &AppState,
    my_generation: u64,
) -> Result<RecommendedWatcher, String> {
    let sqlite_conn = Arc::clone(&state.sqlite_conn);
    let index_dirty = state.index_dirty.clone();
    let vault_generation = Arc::clone(&state.vault_generation);
    let vault_operation = Arc::clone(&state.vault_operation);
    let (tx, rx) = mpsc::channel::<Vec<String>>();
    let root_for_cb = vault_root.clone();
    // Some systems report event paths in their resolved form, such as macOS under /private, or
    // for a vault opened through a symlink. Match those against the resolved root as well.
    let canonical_root = std::fs::canonicalize(&vault_root).ok();

    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let Ok(event) = res else {
                return;
            };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }

            let mut changed_rel_paths = Vec::new();
            for path in &event.paths {
                let Some(rel) = path
                    .strip_prefix(&root_for_cb)
                    .ok()
                    .or_else(|| canonical_root.as_deref().and_then(|root| path.strip_prefix(root).ok()))
                else {
                    continue;
                };
                // Ignore .obsidian, .git, and temporary atomic write dot-files
                if rel
                    .components()
                    .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with('.')))
                {
                    continue;
                }
                let rel_str = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if rel_str.is_empty() {
                    continue;
                }
                if !changed_rel_paths.contains(&rel_str) {
                    changed_rel_paths.push(rel_str);
                }
            }

            if !changed_rel_paths.is_empty() {
                let _ = tx.send(changed_rel_paths);
            }
        })
        .map_err(|e| format!("Failed to create filesystem watcher: {e}"))?;

    watcher
        .watch(&vault_root, RecursiveMode::Recursive)
        .map_err(|e| {
            format!(
                "Failed to watch vault directory {}: {e}",
                vault_root.display()
            )
        })?;

    // Background debounce worker (300 ms quiet window, batched)
    let root_for_worker = vault_root;
    let recent_writes_for_worker = state.recent_self_writes.clone();
    std::thread::spawn(move || {
        let debounce_window = Duration::from_millis(300);
        let max_batch_wait = Duration::from_millis(1200);

        while let Ok(initial_paths) = rx.recv() {
            let batch_start = Instant::now();
            let mut batched = initial_paths;

            while batch_start.elapsed() < max_batch_wait {
                match rx.recv_timeout(debounce_window) {
                    Ok(more_paths) => {
                        for p in more_paths {
                            if !batched.contains(&p) {
                                batched.push(p);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            let _operation = vault_operation.lock_recover();
            if vault_generation.load(Ordering::SeqCst) != my_generation {
                return;
            }
            batched.retain(|path| !is_recent_self_write(&recent_writes_for_worker, &root_for_worker, path));
            if batched.is_empty() {
                continue;
            }
            let search_index = app_handle.state::<AppState>().search_index.lock_recover().clone();
            process_watcher_batch(&root_for_worker, &sqlite_conn, &search_index, &index_dirty, &batched);
            let _ = app_handle.emit(
                "vault-fs-event",
                VaultFsEvent {
                    generation: my_generation,
                    changed_paths: batched,
                },
            );
        }
    });

    Ok(watcher)
}

#[tauri::command]
fn copy_to_clipboard(req: CopyRequest) -> Result<ClipboardPayload, String> {
    let payload = build_clipboard_payload(&req.text, req.mode);
    write_to_system_clipboard(&payload)?;
    Ok(payload)
}

#[tauri::command]
fn preview_clipboard_payload(req: CopyRequest) -> ClipboardPayload {
    build_clipboard_payload(&req.text, req.mode)
}

#[tauri::command]
fn paste_plain_from_clipboard() -> Result<String, String> {
    read_plain_from_system_clipboard()
}

/// Save-dialog text in the UI language. The frontend translates it; Rust keeps English fallbacks.
#[derive(serde::Deserialize)]
struct DialogLabels {
    title: String,
    filter: String,
}

impl DialogLabels {
    fn new(title: &str, filter: &str) -> Self {
        Self { title: title.into(), filter: filter.into() }
    }
}

#[tauri::command]
async fn pick_vault_folder(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file()
            .set_title(title.unwrap_or_else(|| "Select a Markdown vault folder".into()))
            .blocking_pick_folder()
            .map(|p| p.into_path().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string()))
            .transpose()
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn open_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<VaultSnapshot, String> {
    let _operation = state.vault_operation.lock_recover();
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Vault path cannot be empty".to_string());
    }
    let vault_root = PathBuf::from(trimmed);
    let generation = state.next_vault_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let search_index = Arc::new(VaultSearchIndex::new()?);
    let watcher = start_vault_watcher(app_handle, vault_root.clone(), &state, generation)?;
    let mut snapshot = scan_vault_snapshot(&vault_root)?;
    let scanned_files = scan_dir_files_from_disk(&vault_root, &vault_root)?;

    // Each vault keeps its own SQLite index cache (and edit history).
    let app_data_dir = state
        .recent_vaults_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    let db_path = vault_index_db_path(&app_data_dir, &vault_root);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let vault_conn = Connection::open(&db_path)
        .or_else(|_| Connection::open_in_memory())
        .map_err(|e| format!("Failed to open vault index database: {e}"))?;
    search_index.rebuild_from_scanned(&scanned_files)?;
    apply_scanned_vault_index(&vault_conn, &scanned_files)?;

    // Publish only after all fallible setup completes. Watcher batches wait on vault_operation.
    let mut conn = state.sqlite_conn.lock_recover();
    let mut search = state.search_index.lock_recover();
    *conn = vault_conn;
    state.sqlite_conn.clear_poison();
    *search = search_index;
    state.index_dirty.store(false, Ordering::Relaxed);
    state.recent_self_writes.lock_recover().clear();
    state.vault_generation.store(generation, Ordering::SeqCst);
    *state.active_vault.lock_recover() = Some(vault_root.clone());
    *state.watcher.lock_recover() = Some(watcher);
    snapshot.generation = Some(generation);
    drop(search);
    drop(conn);

    // Notes deleted a month ago or more give their history back to the disk.
    if let Ok(_write_lock) = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir, &vault_root) {
        crate::versions::prune_orphans(
            &crate::versions::versions_dir(&app_data_dir, &vault_root),
            &vault_root,
            system_time_to_ms(SystemTime::now()),
        );
    }

    let _ = record_recent_vault(&state.recent_vaults_file, &vault_root);
    Ok(snapshot)
}

#[tauri::command(async)]
fn create_new_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    parent_path: String,
    vault_name: String,
) -> Result<VaultSnapshot, String> {
    validate_entry_name(&vault_name)?;
    let clean_name = vault_name.trim_start();

    let parent = PathBuf::from(parent_path.trim());
    if !parent.exists() || !parent.is_dir() {
        return Err(format!(
            "Parent directory does not exist: {}",
            parent.display()
        ));
    }

    let vault_root = parent.join(clean_name);
    std::fs::create_dir_all(&vault_root).map_err(|e| {
        format!(
            "Failed to create vault directory {}: {e}",
            vault_root.display()
        )
    })?;

    // Seed a starter Welcome.md note only if the newly created folder is completely empty
    {
        let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
        let welcome_path = vault_root.join("Welcome.md");
        if !welcome_path.exists() {
            let initial_welcome = format!(
                "# Welcome to {clean_name}\n\nThis vault is stored as plain `.md` files on disk.\n\n- Press **Ctrl+N** to create a new note\n- Press **Ctrl+C** to copy with Notepad-identical line breaks\n- Press **Ctrl+Shift+V** to paste as plain text\n"
            );
            let _ = crate::fs_atomic::atomic_write_str(&welcome_path, &initial_welcome);
        }
    }

    open_vault(app_handle, state, vault_root.to_string_lossy().into_owned())
}

#[tauri::command(async)]
fn get_vault_snapshot(state: State<'_, AppState>) -> Result<Option<VaultSnapshot>, String> {
    let _operation = state.vault_operation.lock_recover();
    let root_opt = state
        .active_vault
        .lock_recover()
        .clone();
    match root_opt {
        Some(root) => Ok(Some(scan_vault_snapshot(&root)?)),
        None => Ok(None),
    }
}

#[tauri::command(async)]
fn close_vault(state: State<'_, AppState>) -> Result<(), String> {
    let _operation = state.vault_operation.lock_recover();
    state.vault_generation.store(0, Ordering::SeqCst);
    *state.watcher.lock_recover() = None;
    *state.active_vault.lock_recover() = None;
    Ok(())
}

#[tauri::command]
fn get_recent_vaults(state: State<'_, AppState>) -> Result<Vec<RecentVault>, String> {
    Ok(load_recent_vaults(&state.recent_vaults_file))
}

#[tauri::command]
fn remove_recent_vault_entry(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<RecentVault>, String> {
    Ok(remove_recent_vault(&state.recent_vaults_file, &path))
}

#[tauri::command(async)]
fn read_note(state: State<'_, AppState>, rel_path: String) -> Result<NoteDocument, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    read_vault_note(&vault_root, &rel_path)
}

/// Reads `rel_path` back from disk and updates the note index and the search index. The note is
/// already written when this runs, so a failure is logged and never turns a finished write into
/// an error the page would show as a failed save.
fn reindex_note(state: &AppState, vault_root: &Path, rel_path: &str, in_app_save_ms: Option<i64>) {
    let result = (|| -> Result<(), String> {
        let scanned = scan_single_file_from_disk(vault_root, &vault_root.join(rel_path))?;
        let sqlite_result = upsert_scanned_note(&state.sqlite_conn.lock_recover(), &scanned, in_app_save_ms);
        let search_result = state.search_index.lock_recover().upsert_scanned_note(&scanned);
        sqlite_result.and(search_result)
    })();
    if let Err(err) = result {
        state.index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Index update failed for {rel_path}: {err}");
    }
}

/// Stores the text a note holds now in its version history, before a write replaces it. Does
/// nothing when history is off or the note does not exist yet. A failed snapshot never stops the write.
fn keep_previous_text(state: &AppState, vault_root: &Path, rel_path: &str, min_interval_ms: i64) {
    if !state.version_history.load(Ordering::Relaxed) {
        return;
    }
    let Ok((note_path, clean)) = crate::versions::resolve_note_path(vault_root, rel_path) else { return };
    if let Ok(previous) = std::fs::read_to_string(&note_path) {
        let dir = crate::versions::versions_dir(&app_data_dir_of(state), vault_root);
        let _ = crate::versions::snapshot(&dir, &clean, &previous, system_time_to_ms(SystemTime::now()), min_interval_ms);
    }
}

#[tauri::command(async)]
fn create_note(
    state: State<'_, AppState>,
    rel_path: String,
    initial_content: Option<String>,
) -> Result<(VaultSnapshot, NoteDocument), String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let content = initial_content.unwrap_or_default();
    let (snapshot, doc) = create_vault_note(&vault_root, &rel_path, &content)?;
    record_self_write_content(&state.recent_self_writes, &doc.rel_path, &content);
    drop(write_lock);
    reindex_note(&state, &vault_root, &doc.rel_path, Some(system_time_to_ms(SystemTime::now())));

    Ok((snapshot, doc))
}

#[derive(serde::Deserialize)]
struct ExpectedNoteContent {
    content: Option<String>,
}

#[tauri::command(async)]
fn save_note(
    state: State<'_, AppState>,
    rel_path: String,
    content: String,
    vault_generation: u64,
    expected: ExpectedNoteContent,
) -> Result<Option<NoteDocument>, String> {
    let _operation = state.vault_operation.lock_recover();
    if vault_generation != state.vault_generation.load(Ordering::SeqCst) {
        return Err("The vault changed before the note could be saved.".into());
    }
    let vault_root = require_active_vault(&state)?;
    let write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let Some((doc, changed)) = crate::vault::save_vault_note_if_current(
        &vault_root, &rel_path, expected.content.as_deref(), &content,
    )? else {
        return Ok(None);
    };
    if changed && state.version_history.load(Ordering::Relaxed) {
        if let Some(previous) = expected.content.as_deref() {
            let _ = crate::versions::snapshot(
                &versions_dir(&state, &vault_root),
                &doc.rel_path,
                previous,
                system_time_to_ms(SystemTime::now()),
                crate::versions::SNAPSHOT_INTERVAL_MS,
            );
        }
    }
    if changed {
        record_self_write_content(&state.recent_self_writes, &doc.rel_path, &content);
        drop(write_lock);
        reindex_note(&state, &vault_root, &doc.rel_path, Some(system_time_to_ms(SystemTime::now())));
    }

    Ok(Some(doc))
}

#[tauri::command(async)]
fn create_folder(state: State<'_, AppState>, rel_path: String) -> Result<VaultSnapshot, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let clean = crate::vault::normalize_rel_path(&rel_path);
    mark_self_write(&state.recent_self_writes, &clean);
    create_vault_folder(&vault_root, &clean)
}

#[tauri::command(async)]
fn rename_entry(
    state: State<'_, AppState>,
    old_rel_path: String,
    new_rel_path: String,
) -> Result<RenameEntryResult, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let res = rename_vault_entry(&vault_root, &old_rel_path, &new_rel_path)?;
    mark_self_write(&state.recent_self_writes, &res.old_rel_path);
    mark_self_write(&state.recent_self_writes, &res.new_rel_path);

    // Incrementally scan renamed destination outside the SQLite/Tantivy mutexes
    let new_abs = vault_root.join(&res.new_rel_path);
    let single_scanned = if new_abs.is_file() {
        scan_single_file_from_disk(&vault_root, &new_abs).ok()
    } else {
        None
    };
    let dir_scanned = if new_abs.is_dir() {
        scan_dir_files_from_disk(&vault_root, &new_abs).ok()
    } else {
        None
    };

    // The rename is on disk, so a failure below is logged: reporting it would tell the page the
    // rename failed when the file has already moved.
    let reindex = (|| -> Result<(), String> {
        {
            let conn = state.sqlite_conn.lock_recover();
            rename_edit_log_paths(&conn, &res.old_rel_path, &res.new_rel_path)?;
            delete_indexed_path(&conn, &res.old_rel_path)?;
            if let Some(scanned) = &single_scanned {
                upsert_scanned_note(&conn, scanned, None)?;
            } else if let Some(scanned_list) = &dir_scanned {
                apply_incremental_index(&conn, &[], scanned_list, None)?;
            }
        }
        let renamed_files = match (&single_scanned, &dir_scanned) {
            (Some(note), _) => std::slice::from_ref(note),
            (_, Some(notes)) => notes.as_slice(),
            _ => &[],
        };
        state.search_index.lock_recover()
            .apply_changes(std::slice::from_ref(&res.old_rel_path), renamed_files)?;
        Ok(())
    })();
    if let Err(err) = reindex {
        state.index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Index update after rename failed: {err}");
    }

    // Version history follows the note, the same way pins do.
    let _ = crate::versions::rename(
        &versions_dir(&state, &vault_root),
        &res.old_rel_path,
        &res.new_rel_path,
    );

    // A pinned note keeps its pin through a rename or a move, including inside a renamed folder.
    let file = pins_file(&state, &vault_root);
    let pins = crate::pins::load_pins(&file);
    let moved = crate::pins::rename_pins(&pins, &res.old_rel_path, &res.new_rel_path);
    if moved != pins {
        if let Err(err) = crate::pins::save_pins(&file, &vault_root, &moved) {
            eprintln!("Cannot move pins after rename: {err}");
        }
    }

    Ok(res)
}

#[tauri::command(async)]
fn delete_entry(state: State<'_, AppState>, rel_path: String) -> Result<VaultSnapshot, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let clean = crate::vault::normalize_rel_path(&rel_path);
    mark_self_write(&state.recent_self_writes, &clean);

    let snapshot = delete_vault_entry(&vault_root, &clean)?;
    // The entry is in the trash, so an index failure is logged instead of reported as a failed delete.
    let conn = state.sqlite_conn.lock_recover();
    if let Err(err) = delete_indexed_path(&conn, &clean) {
        state.index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Index update after delete failed: {err}");
    }
    drop(conn);
    if let Err(err) = state.search_index.lock_recover().delete_path_or_prefix(&clean) {
        state.index_dirty.store(true, Ordering::Relaxed);
        eprintln!("Search update after delete failed: {err}");
    }

    Ok(snapshot)
}

#[tauri::command(async)]
fn query_calendar_days(
    state: State<'_, AppState>,
    year_month: String,
    field: CalendarDateField,
) -> Result<Vec<CalendarDaySummary>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    query_calendar_month(&conn, &year_month, field)
}

#[tauri::command(async)]
fn query_calendar_date_notes(
    state: State<'_, AppState>,
    date: String,
    field: CalendarDateField,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    query_notes_on_date(&conn, &date, field)
}

#[tauri::command(async)]
fn get_backlinks(
    state: State<'_, AppState>,
    rel_path: String,
) -> Result<Vec<BacklinkEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    query_backlinks_for_note(&conn, &rel_path)
}

#[tauri::command(async)]
fn get_all_tags(state: State<'_, AppState>) -> Result<Vec<TagSummary>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    query_all_tags(&conn)
}

#[tauri::command(async)]
fn get_notes_by_tag(
    state: State<'_, AppState>,
    tag: String,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    query_notes_by_tag(&conn, &tag)
}

/// Runs a search on a worker thread. It reads only the index, never a note file, so it needs no
/// vault lock. A newer search cancels an older one that is still running, which then returns
/// `search-cancelled`.
#[tauri::command(async)]
fn search_vault(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let index = state.search_index.lock_recover().clone();
    index.search(&query, limit.unwrap_or(40))
}

#[tauri::command(async)]
fn rebuild_vault_index(state: State<'_, AppState>) -> Result<usize, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    rebuild_indexes(&state, &vault_root)
}

/// The caller holds vault_operation so both indexes belong to the same vault.
fn rebuild_indexes(state: &AppState, vault_root: &Path) -> Result<usize, String> {
    let scanned_files = scan_dir_files_from_disk(vault_root, vault_root)?;
    let search = Arc::new(VaultSearchIndex::new()?);
    search.rebuild_from_scanned(&scanned_files)?;
    let conn = state.sqlite_conn.lock_recover();
    apply_scanned_vault_index(&conn, &scanned_files)?;
    *state.search_index.lock_recover() = search;
    // Repair the connection after a panic. The rebuild succeeded on this connection.
    state.sqlite_conn.clear_poison();
    drop(conn);
    state.index_dirty.store(false, Ordering::Relaxed);
    Ok(scanned_files.len())
}

#[tauri::command(async)]
fn save_pasted_image(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<(String, String), String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    // The page sends the image as the raw request body, which avoids encoding every byte as a
    // JSON number. An empty body asks for the image on the system clipboard instead.
    let (file_name, rel_path) = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) if !bytes.is_empty() => {
            crate::media_export::save_png_to_vault_attachments(&vault_root, bytes, None)?
        }
        _ => crate::media_export::paste_clipboard_image_to_vault(&vault_root)?,
    };
    mark_self_write(&state.recent_self_writes, &rel_path);
    Ok((file_name, rel_path))
}

#[tauri::command(async)]
fn read_vault_image_data_url(state: State<'_, AppState>, target: String) -> Result<String, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    crate::media_export::read_vault_image_as_data_url(&vault_root, &target)
}

/// Asks where to save and writes `html`, the export built by `render_note_html_preview` with
/// math and diagrams rendered by the frontend. Returns `None` when the user cancels.
#[tauri::command]
async fn export_active_note_html(
    app: AppHandle,
    title: String,
    html: String,
    labels: Option<DialogLabels>,
) -> Result<Option<String>, String> {
    let labels = labels.unwrap_or_else(|| DialogLabels::new("Export note to HTML", "HTML document"));
    tauri::async_runtime::spawn_blocking(move || {
        let name = crate::platform::export_file_name(&title, "html");
        let Some(path) = app.dialog().file().set_title(labels.title)
            .set_file_name(name).add_filter(labels.filter, &["html"])
            .blocking_save_file() else { return Ok(None); };
        let path = path.into_path().map_err(|e| e.to_string())?;
        crate::media_export::export_html_to_path(&path, &html).map(Some)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn query_due_tasks(state: State<'_, AppState>) -> Result<Vec<TaskEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    crate::index::query_due_tasks(&conn)
}

/// Finds the links that a rename or move of a note would break and says how to rewrite them.
#[tauri::command]
async fn plan_link_updates(
    state: State<'_, AppState>,
    old_rel_path: String,
    new_rel_path: String,
) -> Result<crate::link_update::LinkUpdatePlan, String> {
    let vault_root = require_active_vault(&state)?;
    let operation = Arc::clone(&state.vault_operation);
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation.lock_recover();
        crate::link_update::plan_link_updates(&vault_root, &old_rel_path, &new_rel_path)
    })
    .await
    .map_err(|e| format!("Link scan failed: {e}"))?
}

/// Applies planned link edits to a note that is not open in the editor. The write is not marked
/// as the app's own, so the file watcher refreshes the note if a tab has it open.
#[tauri::command(async)]
fn apply_link_edits(
    state: State<'_, AppState>,
    rel_path: String,
    edits: Vec<crate::link_update::TextEdit>,
) -> Result<crate::link_update::AppliedEdits, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    // The rewritten note keeps its previous text in version history, as it does for the CLI.
    keep_previous_text(&state, &vault_root, &rel_path, 0);
    let applied = crate::link_update::apply_edits_to_note(&vault_root, &rel_path, &edits)?;
    if applied.applied > 0 {
        let rel = crate::vault::ensure_md_extension(&rel_path);
        reindex_note(&state, &vault_root, &rel, None);
    }
    Ok(applied)
}

/// Scans the open vault for broken links, unused attachments, empty notes, and duplicate titles.
#[tauri::command]
async fn get_vault_health(
    state: State<'_, AppState>,
    templates_folder: String,
) -> Result<crate::health::HealthReport, String> {
    let vault_root = require_active_vault(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::health::scan_vault_health(&vault_root, &templates_folder)
    })
    .await
    .map_err(|e| format!("Vault health scan failed: {e}"))?
}

#[tauri::command(async)]
fn get_note_cards(state: State<'_, AppState>) -> Result<Vec<NoteCard>, String> {
    let conn = state
        .sqlite_conn
        .lock_recover();
    crate::index::query_note_cards(&conn)
}

/// The app data folder, beside the recent vaults list.
fn app_data_dir_of(state: &AppState) -> PathBuf {
    state
        .recent_vaults_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
}

fn app_data_dir(state: &State<'_, AppState>) -> PathBuf {
    app_data_dir_of(state)
}

/// Returns the saved app settings as JSON, or `None` before the first save.
#[tauri::command]
fn load_app_settings(state: State<'_, AppState>) -> Result<Option<String>, String> {
    crate::app_settings::load(&crate::app_settings::settings_path(&app_data_dir(&state)))
}

#[tauri::command]
fn save_app_settings(state: State<'_, AppState>, json: String) -> Result<(), String> {
    crate::app_settings::save(&crate::app_settings::settings_path(&app_data_dir(&state)), &json)
}

/// Asks where to save the settings export and writes `json` there. Returns `None` on cancel.
#[tauri::command]
async fn export_settings_file(
    app: AppHandle,
    json: String,
    labels: Option<DialogLabels>,
) -> Result<Option<String>, crate::app_settings::SettingsFileError> {
    let labels = labels.unwrap_or_else(|| DialogLabels::new("Export settings", "Jotter settings"));
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app.dialog().file().set_title(labels.title)
            .set_file_name("jotter-settings.json").add_filter(labels.filter, &["json"])
            .blocking_save_file() else { return Ok(None); };
        let path = path.into_path().map_err(|e| crate::app_settings::SettingsFileError::new("invalid_path", Path::new(""), e))?;
        crate::app_settings::save(&path, &json).map_err(|e| crate::app_settings::SettingsFileError::new("write", &path, e))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await.map_err(|e| crate::app_settings::SettingsFileError::new("operation", Path::new(""), e))?
}

/// Asks for a settings file and returns its text. Returns `None` on cancel.
#[tauri::command]
async fn import_settings_file(
    app: AppHandle,
    labels: Option<DialogLabels>,
) -> Result<Option<String>, crate::app_settings::SettingsFileError> {
    let labels = labels.unwrap_or_else(|| DialogLabels::new("Import settings", "Jotter settings"));
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app.dialog().file().set_title(labels.title)
            .add_filter(labels.filter, &["json"]).blocking_pick_file() else { return Ok(None); };
        let path = path.into_path().map_err(|e| crate::app_settings::SettingsFileError::new("invalid_path", Path::new(""), e))?;
        crate::app_settings::read_import(&path).map(Some)
    }).await.map_err(|e| crate::app_settings::SettingsFileError::new("operation", Path::new(""), e))?
}

/// Turns task reminders on or off.
#[tauri::command]
fn configure_task_reminders(
    state: State<'_, crate::reminders::ReminderState>,
    enabled: bool,
) {
    state.enabled.store(enabled, Ordering::SeqCst);
}

/// Sets the open vault's backup schedule, or turns scheduled backups off with `None`.
#[tauri::command]
fn configure_backups(
    app: AppHandle,
    state: State<'_, crate::backup::BackupState>,
    config: Option<crate::backup::BackupConfig>,
) -> Result<(), String> {
    if let Some(config) = &config {
        if config.folder.trim().is_empty() {
            return Err("Choose a folder for the backups".to_string());
        }
    }
    let enabled = config.is_some();
    *state.config.lock_recover() = config;
    *state.last_error.lock_recover() = None;
    // Don't make the first backup wait for the next minute tick.
    if enabled {
        std::thread::spawn(move || run_due_backup(&app));
    }
    Ok(())
}

#[tauri::command]
fn get_backup_status(
    state: State<'_, AppState>,
    backups: State<'_, crate::backup::BackupState>,
) -> Result<crate::backup::BackupStatus, String> {
    let vault_root = require_active_vault(&state)?;
    Ok(crate::backup::BackupStatus {
        last_backup_ms: crate::backup::last_backup_ms(&crate::backup::record_path(&app_data_dir(&state), &vault_root)),
        last_error: backups.last_error.lock_recover().clone(),
    })
}

/// Backs up the open vault now, using the configured folder. Returns the new file's path.
#[tauri::command]
async fn run_backup_now(
    state: State<'_, AppState>,
    backups: State<'_, crate::backup::BackupState>,
) -> Result<String, String> {
    let vault_root = require_active_vault(&state)?;
    let config = backups
        .config
        .lock_recover()
        .clone()
        .filter(|c| c.vault == vault_root.to_string_lossy())
        .ok_or_else(|| "Choose a folder for the backups".to_string())?;
    let record = crate::backup::record_path(&app_data_dir(&state), &vault_root);
    let running = backups.running.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let _running = running.lock_recover();
        crate::backup::run(&vault_root, &config, &record, chrono::Local::now())
    })
    .await
    .map_err(|e| e.to_string())?;
    *backups.last_error.lock_recover() = outcome.as_ref().err().cloned();
    outcome.map(|p| p.to_string_lossy().into_owned())
}

/// Asks which folder should hold the backups. Returns `None` when the user cancels.
#[tauri::command]
async fn pick_backup_folder(app: AppHandle, title: Option<String>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file()
            .set_title(title.unwrap_or_else(|| "Choose a backup folder".into()))
            .blocking_pick_folder()
            .map(|p| p.into_path().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string()))
            .transpose()
    }).await.map_err(|e| e.to_string())?
}

/// Once a minute, backs up the open vault when its schedule says a backup is due. Runs for the
/// life of the app, so backups still happen while Jotter sits in the tray.
fn spawn_backup_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        run_due_backup(&app);
    });
}

/// Backs up the open vault if its schedule says one is due. A vault with no backup yet is
/// always due, so opening a vault with backups on, or turning them on, backs it up at once.
/// Backups never overlap: a second caller waits for the first and then finds nothing due.
fn run_due_backup(app: &AppHandle) {
    let state = app.state::<AppState>();
    let backups = app.state::<crate::backup::BackupState>();
    let _running = backups.running.lock_recover();
    let Some(config) = backups.config.lock_recover().clone() else { return };
    let Some(root) = state.active_vault.lock_recover().clone() else { return };
    if config.vault != root.to_string_lossy() {
        return;
    }
    let record = crate::backup::record_path(&app_data_dir(&state), &root);
    let now = chrono::Local::now();
    if !crate::backup::is_due(crate::backup::last_backup_ms(&record), config.interval_hours, now.timestamp_millis()) {
        return;
    }
    let outcome = crate::backup::run(&root, &config, &record, now);
    if let Err(err) = &outcome {
        eprintln!("Scheduled backup failed: {err}");
    }
    *backups.last_error.lock_recover() = outcome.err();
}

fn spawn_index_repair_loop(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let state = app.state::<AppState>();
        if !state.index_dirty.load(Ordering::Relaxed) && !state.sqlite_conn.is_poisoned() {
            continue;
        }
        let _operation = state.vault_operation.lock_recover();
        if !state.index_dirty.load(Ordering::Relaxed) && !state.sqlite_conn.is_poisoned() {
            continue;
        }
        let Ok(root) = require_active_vault(&state) else { continue };
        if let Err(err) = rebuild_indexes(&state, &root) {
            eprintln!("Cannot repair vault index: {err}");
            continue;
        }
        let _ = app.emit("vault-fs-event", VaultFsEvent {
            generation: state.vault_generation.load(Ordering::SeqCst),
            changed_paths: Vec::new(),
        });
    });
}

/// Checks the open vault's tasks every 30 seconds and shows a notification for each one that
/// has come due. Runs for the life of the app, so reminders still fire while Jotter sits in
/// the tray.
fn spawn_reminder_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let reminders = app.state::<crate::reminders::ReminderState>();
        let file = crate::reminders::reminders_path(&app_data_dir(&state));
        let mut announced = crate::reminders::load_announced(&file);
        let mut history_dirty = file.exists();
        crate::reminders::prune_announced(&mut announced, chrono::Utc::now());
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let now = chrono::Local::now();
            history_dirty |= crate::reminders::prune_announced(&mut announced, now.with_timezone(&chrono::Utc));
            if history_dirty {
                match crate::reminders::save_announced(&file, &mut announced, now.with_timezone(&chrono::Utc)) {
                    Ok(()) => history_dirty = false,
                    Err(err) => eprintln!("Cannot save task reminders: {err}"),
                }
            }
            if !reminders.enabled.load(Ordering::SeqCst) {
                continue;
            }
            // Serialize with vault open/close so the database and vault identity belong together.
            // Keep this guard through notification submission; SQLite is released after the query.
            let _operation = state.vault_operation.lock_recover();
            let root = state.active_vault.lock_recover().clone();
            let Some(root) = root else { continue };
            let tasks = match crate::reminders::query_tasks(&state.sqlite_conn, &root) {
                Ok(tasks) => tasks,
                Err(err) => { eprintln!("Cannot query task reminders: {err}"); continue; }
            };
            let vault_key = crate::platform::vault_key(&root);
            let due = crate::reminders::due_reminders(Some(&vault_key), &tasks, now, &announced);
            if due.is_empty() {
                continue;
            }
            for reminder in due {
                let shown = app.notification().builder()
                    .title(&reminder.title)
                    .body(&reminder.body)
                    .show();
                // Keep an unshown reminder for the next pass, since the system may be busy.
                if shown.is_ok() {
                    announced.insert(reminder.key, reminder.due);
                    history_dirty = true;
                }
            }
            if history_dirty {
                match crate::reminders::save_announced(&file, &mut announced, now.with_timezone(&chrono::Utc)) {
                    Ok(()) => history_dirty = false,
                    Err(err) => eprintln!("Cannot save task reminders: {err}"),
                }
            }
        }
    });
}

/// Asks where to keep the task calendar file. Returns `None` when the user cancels.
#[tauri::command]
async fn pick_task_calendar_path(
    app: AppHandle,
    default_name: String,
    labels: Option<DialogLabels>,
) -> Result<Option<String>, String> {
    let labels = labels.unwrap_or_else(|| DialogLabels::new("Save task calendar", "iCalendar file"));
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(labels.title)
            .set_file_name(crate::platform::export_file_name(&default_name, "ics"))
            .add_filter(labels.filter, &["ics"]).blocking_save_file()
            .map(|p| p.into_path().map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string()))
            .transpose()
    }).await.map_err(|e| e.to_string())?
}

/// Writes the open vault's due-dated tasks to an `.ics` file. Skips the write when only the
/// timestamp would change, so sync clients and subscribed calendars see no churn.
/// Returns whether the file changed.
#[tauri::command(async)]
fn write_task_calendar(state: State<'_, AppState>, target_path: String) -> Result<bool, String> {
    let operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let target = PathBuf::from(target_path.trim());
    if target.as_os_str().is_empty() {
        return Err("Choose a file for the task calendar".to_string());
    }
    let tasks = {
        let conn = state
            .sqlite_conn
            .lock_recover();
        crate::index::query_due_tasks(&conn)?
    };
    drop(operation);
    let name = vault_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Jotter".to_string());
    let ics = crate::tasks::build_ics(&format!("{name} tasks"), &tasks, system_time_to_ms(SystemTime::now()));
    let without_stamp = |text: &str| -> String {
        text.lines().filter(|l| !l.starts_with("DTSTAMP:")).collect::<Vec<_>>().join("\n")
    };
    if let Ok(existing) = std::fs::read_to_string(&target) {
        if without_stamp(&existing) == without_stamp(&ics) {
            return Ok(false);
        }
    }
    crate::fs_atomic::atomic_write_str(&target, &ics)
        .map_err(|e| format!("Cannot write {}: {e}", target.display()))?;
    Ok(true)
}

/// Keychain account for a vault's CalDAV password.
fn caldav_account(vault_root: &Path) -> String {
    format!("caldav:{}", crate::platform::vault_key(vault_root))
}

/// Stores the CalDAV password for the open vault in the system credential store. The password
/// never returns to the webview.
#[tauri::command]
fn caldav_set_password(state: State<'_, AppState>, password: String) -> Result<(), String> {
    use crate::ai::secrets::{OsSecretStore, SecretStore};
    let vault_root = require_active_vault(&state)?;
    if password.is_empty() {
        return Err("Enter the calendar password".to_string());
    }
    OsSecretStore.set(&caldav_account(&vault_root), &password)
}

#[tauri::command]
fn caldav_has_password(state: State<'_, AppState>) -> Result<bool, String> {
    use crate::ai::secrets::{OsSecretStore, SecretStore};
    let vault_root = require_active_vault(&state)?;
    Ok(OsSecretStore.get(&caldav_account(&vault_root))?.is_some())
}

/// Turns CalDAV off for the open vault: removes the password and the record of pushed events.
/// Events already on the server stay there.
#[tauri::command]
fn caldav_forget(state: State<'_, AppState>) -> Result<(), String> {
    use crate::ai::secrets::{OsSecretStore, SecretStore};
    let vault_root = require_active_vault(&state)?;
    OsSecretStore.delete(&caldav_account(&vault_root))?;
    match std::fs::remove_file(crate::caldav::state_path(&app_data_dir(&state), &vault_root)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Cannot remove the CalDAV record: {e}")),
    }
}

/// Pushes the open vault's open tasks to the CalDAV calendar and returns what changed.
#[tauri::command]
async fn caldav_sync(
    state: State<'_, AppState>,
    calendar_url: String,
    username: String,
) -> Result<crate::caldav::SyncReport, String> {
    use crate::ai::secrets::{OsSecretStore, SecretStore};
    let vault_root = require_active_vault(&state)?;
    let collection = crate::caldav::normalize_collection(&calendar_url)?;
    let password = OsSecretStore
        .get(&caldav_account(&vault_root))?
        .ok_or_else(|| "Enter the calendar password".to_string())?;
    let tasks = {
        let conn = state
            .sqlite_conn
            .lock_recover();
        crate::index::query_due_tasks(&conn)?
    };
    let now_ms = system_time_to_ms(SystemTime::now());
    let events: std::collections::BTreeMap<String, String> = crate::tasks::open_task_uids(&tasks)
        .into_iter()
        .map(|(uid, task)| {
            let body = crate::tasks::build_event_ics(&uid, task, now_ms);
            (uid, body)
        })
        .collect();

    let state_file = crate::caldav::state_path(&app_data_dir(&state), &vault_root);
    let previous = crate::caldav::load_state(&state_file);
    let (next, report) = crate::caldav::sync(&collection, username.trim(), &password, &events, &previous).await?;
    if next != previous {
        crate::caldav::save_state(&state_file, &next)?;
    }
    Ok(report)
}

#[tauri::command(async)]
fn render_note_html_preview(
    state: State<'_, AppState>,
    title: String,
    markdown: String,
    dark_mode: bool,
) -> Result<String, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = state
        .active_vault
        .lock_recover()
        .clone();
    Ok(crate::media_export::build_self_contained_html(
        &title,
        &markdown,
        vault_root.as_deref(),
        dark_mode,
    ))
}

#[tauri::command]
async fn list_deleted_entries(state: State<'_, AppState>) -> Result<Vec<DeletedEntry>, String> {
    let vault_root = require_active_vault(&state)?;
    tauri::async_runtime::spawn_blocking(move || crate::recycle::list_deleted(&vault_root))
        .await
        .map_err(|e| format!("Could not read the Recycle Bin: {e}"))?
}

/// Restores a trashed vault entry to its original path and indexes it. Returns the restored
/// relative path and a fresh snapshot.
#[tauri::command(async)]
fn restore_deleted_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<(String, VaultSnapshot), String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let rel_path = crate::recycle::restore(&vault_root, &id)?;
    mark_self_write(&state.recent_self_writes, &rel_path);

    let abs = vault_root.join(&rel_path);
    let scanned = if abs.is_dir() {
        scan_dir_files_from_disk(&vault_root, &abs)?
    } else {
        vec![scan_single_file_from_disk(&vault_root, &abs)?]
    };
    {
        let conn = state
            .sqlite_conn
            .lock_recover();
        apply_incremental_index(&conn, &[], &scanned, None)?;
    }
    state.search_index.lock_recover().upsert_batch(&scanned)?;

    Ok((rel_path, scan_vault_snapshot(&vault_root)?))
}

fn snippets_dir(state: &State<'_, AppState>) -> PathBuf {
    state
        .recent_vaults_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
        .join("snippets")
}

#[tauri::command]
fn list_css_snippets(state: State<'_, AppState>) -> Result<Vec<CssSnippet>, String> {
    crate::snippets::list_snippets(&snippets_dir(&state))
}

#[tauri::command]
fn open_snippets_folder(state: State<'_, AppState>) -> Result<(), String> {
    crate::snippets::reveal_folder(&snippets_dir(&state))
}

fn spell_paths(app_handle: &AppHandle, state: &State<'_, AppState>) -> crate::spell::SpellPaths {
    crate::spell::SpellPaths {
        bundled: app_handle.path().resource_dir().ok().map(|d| d.join("dictionaries")),
        user: state
            .recent_vaults_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir)
            .join("dictionaries"),
    }
}

/// Pins file for the open vault, beside the recent vaults list in the app data folder.
fn pins_file(state: &State<'_, AppState>, vault_root: &Path) -> PathBuf {
    crate::pins::pins_path(&app_data_dir(state), vault_root)
}

/// Version history folder for the open vault.
fn versions_dir(state: &State<'_, AppState>, vault_root: &Path) -> PathBuf {
    crate::versions::versions_dir(&app_data_dir(state), vault_root)
}

#[tauri::command(async)]
fn list_pinned_notes(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let stored = crate::pins::load_pins(&pins_file(&state, &vault_root));
    Ok(crate::pins::existing_pins(&stored, &vault_root))
}

/// Pins or unpins one note and returns the whole list, so the sidebar never guesses the order.
#[tauri::command(async)]
fn set_note_pinned(
    state: State<'_, AppState>,
    rel_path: String,
    pinned: bool,
) -> Result<Vec<String>, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let clean = crate::vault::normalize_rel_path(&rel_path);
    if pinned && !vault_root.join(&clean).is_file() {
        return Err(format!("Cannot pin {clean}: the note no longer exists"));
    }
    let file = pins_file(&state, &vault_root);
    let next = crate::pins::set_pinned(&crate::pins::load_pins(&file), &clean, pinned);
    crate::pins::save_pins(&file, &vault_root, &next)?;
    Ok(crate::pins::existing_pins(&next, &vault_root))
}

/// Every stored version of a note, newest first.
#[tauri::command(async)]
fn list_note_versions(
    state: State<'_, AppState>,
    rel_path: String,
) -> Result<Vec<crate::versions::NoteVersion>, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let (_, clean) = crate::versions::resolve_note_path(&vault_root, &rel_path)?;
    Ok(crate::versions::list(&versions_dir(&state, &vault_root), &clean))
}

/// The text of one stored version, for the timeline's diff.
#[tauri::command(async)]
fn read_note_version(
    state: State<'_, AppState>,
    rel_path: String,
    id: String,
) -> Result<String, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let (_, clean) = crate::versions::resolve_note_path(&vault_root, &rel_path)?;
    crate::versions::read(&versions_dir(&state, &vault_root), &clean, &id)
}

/// Writes a stored version back to the note, keeping the text it replaces as a new version.
#[tauri::command(async)]
fn restore_note_version(
    state: State<'_, AppState>,
    rel_path: String,
    id: String,
) -> Result<NoteDocument, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    let (note_path, clean) = crate::versions::resolve_note_path(&vault_root, &rel_path)?;
    let dir = versions_dir(&state, &vault_root);
    let wanted = crate::versions::read(&dir, &clean, &id)?;

    // Restoring is itself an edit, so the text on screen becomes a version of its own. It is
    // kept whatever the interval says, so one restore can always be undone by another.
    match std::fs::read_to_string(&note_path) {
        Ok(previous) if previous != wanted => {
            crate::versions::snapshot_before_restore(&dir, &clean, &previous, system_time_to_ms(SystemTime::now()))?;
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Cannot read the current note before restoring: {e}")),
    }

    let (doc, changed) = save_vault_note(&vault_root, &clean, &wanted)?;
    if changed {
        record_self_write_content(&state.recent_self_writes, &doc.rel_path, &wanted);
        reindex_note(&state, &vault_root, &doc.rel_path, Some(system_time_to_ms(SystemTime::now())));
    }
    Ok(doc)
}

/// Turns snapshots on or off. Off, saves keep no history; what is stored stays.
#[tauri::command]
fn configure_version_history(state: State<'_, AppState>, enabled: bool) {
    state.version_history.store(enabled, Ordering::Relaxed);
}

#[tauri::command]
fn list_spell_dictionaries(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    spell: State<'_, Arc<crate::spell::SpellState>>,
) -> Vec<SpellDictionaryInfo> {
    // Settings lists dictionaries when it opens and on Reload, after the user changed files.
    spell.reload();
    crate::spell::list_dictionaries(&spell_paths(&app_handle, &state))
}

/// Returns the subset of `words` that the dictionaries for `languages` reject.
#[tauri::command]
async fn spell_check_words(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    spell: State<'_, Arc<crate::spell::SpellState>>,
    languages: Vec<String>,
    words: Vec<String>,
) -> Result<Vec<String>, String> {
    let paths = spell_paths(&app_handle, &state);
    let spell = Arc::clone(&spell);
    tauri::async_runtime::spawn_blocking(move || spell.misspelled(&paths, &languages, &words))
        .await
        .map_err(|e| format!("Spell check failed: {e}"))?
}

#[tauri::command]
async fn spell_suggest(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    spell: State<'_, Arc<crate::spell::SpellState>>,
    languages: Vec<String>,
    word: String,
) -> Result<Vec<String>, String> {
    let paths = spell_paths(&app_handle, &state);
    let spell = Arc::clone(&spell);
    tauri::async_runtime::spawn_blocking(move || spell.suggest(&paths, &languages, &word))
        .await
        .map_err(|e| format!("Spelling suggestions failed: {e}"))?
}

#[tauri::command]
fn spell_add_word(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    spell: State<'_, Arc<crate::spell::SpellState>>,
    word: String,
) -> Result<(), String> {
    spell.add_personal_word(&spell_paths(&app_handle, &state), &word)
}

#[tauri::command]
fn open_dictionaries_folder(app_handle: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    crate::snippets::reveal_folder(&spell_paths(&app_handle, &state).user)
}

/// Appends text from the quick capture window to the inbox note. The write is not marked as the
/// app's own, so the file watcher tells the main window to reload the note if it has it open.
#[tauri::command(async)]
fn quick_capture_append(
    state: State<'_, AppState>,
    rel_path: String,
    text: String,
    compact: Option<bool>,
) -> Result<String, String> {
    let _operation = state.vault_operation.lock_recover();
    let vault_root = require_active_vault(&state)?;
    let _write_lock = jotter_core::vault_lock::VaultWriteLock::acquire(&app_data_dir_of(&state), &vault_root)?;
    keep_previous_text(&state, &vault_root, &rel_path, crate::versions::SNAPSHOT_INTERVAL_MS);
    let rel = crate::capture::append_to_note(&vault_root, &rel_path, &text, compact.unwrap_or(false))?;
    reindex_note(&state, &vault_root, &rel, None);
    Ok(rel)
}

#[tauri::command]
fn configure_quick_capture(app_handle: AppHandle, enabled: bool, shortcut: String) -> Result<(), String> {
    crate::tray::configure_capture(&app_handle, enabled, &shortcut)
}

#[tauri::command]
fn hide_capture_window(app_handle: AppHandle) {
    crate::tray::hide_capture_window(&app_handle);
}

#[tauri::command]
fn configure_summon(app_handle: AppHandle, enabled: bool, shortcut: String) -> Result<(), String> {
    crate::tray::configure(&app_handle, enabled, &shortcut)
}

/// Shows or hides the tray icon and sets what the window's close button does.
#[tauri::command]
fn configure_window_behavior(
    app_handle: AppHandle,
    tray_resident: bool,
    hide_on_close: bool,
) -> Result<(), String> {
    crate::tray::set_window_behavior(&app_handle, tray_resident, hide_on_close)
}

#[tauri::command]
fn quit_app(app_handle: AppHandle, quit: bool) {
    if quit || !crate::tray::should_hide_window(&app_handle) {
        app_handle.exit(0);
    } else if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn keep_app_open(app_handle: AppHandle) {
    crate::tray::show_main_window(&app_handle);
}

/// Relabels the tray menu in the UI language the frontend chose.
#[tauri::command]
fn set_tray_labels(app_handle: AppHandle, show: String, quit: String) -> Result<(), String> {
    crate::tray::set_labels(&app_handle, &show, &quit).map_err(|e| e.to_string())
}

#[tauri::command]
fn ai_get_status(ai: State<'_, Arc<crate::ai::AiState>>) -> Result<crate::ai::AiStatus, String> {
    ai.status()
}

#[tauri::command]
fn ai_save_config(
    ai: State<'_, Arc<crate::ai::AiState>>,
    config: crate::ai::config::AiConfig,
) -> Result<crate::ai::AiStatus, String> {
    ai.save_config(config)
}

/// The credential store can block, for example on a macOS Keychain prompt.
#[tauri::command]
async fn ai_save_key(
    ai: State<'_, Arc<crate::ai::AiState>>,
    provider: crate::ai::config::AiProvider,
    key: String,
) -> Result<crate::ai::AiStatus, String> {
    let ai = Arc::clone(&ai);
    tauri::async_runtime::spawn_blocking(move || ai.save_key(provider, &key))
        .await
        .map_err(|e| format!("Saving the API key failed: {e}"))?
}

#[tauri::command]
async fn ai_delete_key(
    ai: State<'_, Arc<crate::ai::AiState>>,
    provider: crate::ai::config::AiProvider,
) -> Result<crate::ai::AiStatus, String> {
    let ai = Arc::clone(&ai);
    tauri::async_runtime::spawn_blocking(move || ai.delete_key(provider))
        .await
        .map_err(|e| format!("Removing the API key failed: {e}"))?
}

#[tauri::command]
async fn ai_run(
    ai: State<'_, Arc<crate::ai::AiState>>,
    request: crate::ai::AiRequest,
) -> Result<crate::ai::AiResult, String> {
    let ai = Arc::clone(&ai);
    ai.run(request).await
}

#[tauri::command]
async fn ask_index_status(
    state: State<'_, AppState>,
    ai: State<'_, Arc<crate::ai::AiState>>,
) -> Result<crate::ai::ask::IndexStatus, String> {
    let root = require_active_vault(&state)?;
    let data = app_data_dir(&state);
    let ai = Arc::clone(&ai);
    tauri::async_runtime::spawn_blocking(move || crate::ai::ask::index_status(&ai, &data, &root))
        .await
        .map_err(|e| format!("Checking the vault failed: {e}"))?
}

#[tauri::command]
async fn ask_index_vault(
    state: State<'_, AppState>,
    ai: State<'_, Arc<crate::ai::AiState>>,
) -> Result<crate::ai::ask::IndexReport, String> {
    let root = require_active_vault(&state)?;
    let data = app_data_dir(&state);
    crate::ai::ask::index_vault(&ai, &data, &root).await
}

#[tauri::command]
fn ask_delete_index(state: State<'_, AppState>) -> Result<(), String> {
    let root = require_active_vault(&state)?;
    crate::ai::ask::delete_index(&app_data_dir(&state), &root)
}

#[tauri::command]
async fn ask_vault(
    state: State<'_, AppState>,
    ai: State<'_, Arc<crate::ai::AiState>>,
    question: String,
) -> Result<crate::ai::ask::Answer, String> {
    let root = require_active_vault(&state)?;
    let data = app_data_dir(&state);
    crate::ai::ask::ask(&ai, &data, &root, &question).await
}

#[tauri::command]
fn ai_cancel(ai: State<'_, Arc<crate::ai::AiState>>, operation_id: String) -> bool {
    ai.cancel(&operation_id)
}

/// The `jotter` CLI the installer put next to the app, or `None` when this copy has none,
/// such as an AppImage or a build that skipped the CLI.
#[tauri::command]
fn get_cli_path() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let cli = exe.parent()?.join(format!("jotter{}", std::env::consts::EXE_SUFFIX));
    cli.is_file().then(|| cli.to_string_lossy().into_owned())
}

/// Asks the update feed whether a newer release exists.
#[tauri::command]
async fn check_for_update(
    app_handle: AppHandle,
    state: State<'_, crate::updates::UpdateState>,
) -> Result<Option<crate::updates::UpdateInfo>, String> {
    crate::updates::check(app_handle, state).await
}

/// Downloads the update the last check found, installs it, and restarts Jotter.
/// The page saves open notes first.
#[tauri::command]
async fn install_update(
    app_handle: AppHandle,
    state: State<'_, crate::updates::UpdateState>,
    expected_version: String,
) -> Result<(), String> {
    crate::updates::install(app_handle, state, expected_version).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = crate::platform::app_data_dir().expect("Cannot locate Jotter app data");
    std::fs::create_dir_all(&app_data_dir).expect("Cannot create Jotter app data folder");
    let recent_vaults_file = app_data_dir.join("recent_vaults.json");
    let ai_state = Arc::new(crate::ai::AiState::new(
        crate::ai::config::settings_path(&app_data_dir),
        Arc::new(crate::ai::secrets::OsSecretStore),
    ));

    // Placeholder index until a vault opens; `open_vault` swaps in the vault's own database.
    let conn = Connection::open_in_memory().expect("Failed to open in-memory SQLite index");
    let _ = init_index_schema(&conn);

    let search_idx = VaultSearchIndex::new().expect("Failed to initialize Tantivy search index");

    let builder = tauri::Builder::default();
    // Single instance must be the first plugin, so a second launch exits before it builds state.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        crate::tray::show_main_window(app);
    }));

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    #[cfg(desktop)]
    let builder = builder.plugin(window_state_plugin());

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(crate::updates::UpdateState::default())
        .manage(crate::reminders::ReminderState::default())
        .manage(crate::backup::BackupState::default())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(crate::tray::BackgroundState::default())
        .manage(Arc::new(crate::spell::SpellState::default()))
        .manage(ai_state)
        .manage(AppState {
            active_vault: Mutex::new(None),
            sqlite_conn: Arc::new(Mutex::new(conn)),
            search_index: Mutex::new(Arc::new(search_idx)),
            watcher: Mutex::new(None),
            recent_self_writes: Arc::new(Mutex::new(HashMap::new())),
            recent_vaults_file,
            vault_generation: Arc::new(AtomicU64::new(0)),
            next_vault_generation: AtomicU64::new(0),
            index_dirty: Arc::new(AtomicBool::new(false)),
            vault_operation: Arc::new(Mutex::new(())),
            version_history: Arc::new(AtomicBool::new(true)),
        })
        .setup(|app| {
            crate::tray::create_tray(app)?;
            spawn_reminder_loop(app.handle().clone());
            spawn_backup_loop(app.handle().clone());
            spawn_index_repair_loop(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    crate::tray::request_close(window.app_handle(), false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            copy_to_clipboard,
            preview_clipboard_payload,
            paste_plain_from_clipboard,
            pick_vault_folder,
            open_vault,
            create_new_vault,
            get_vault_snapshot,
            close_vault,
            get_recent_vaults,
            remove_recent_vault_entry,
            read_note,
            create_note,
            save_note,
            create_folder,
            rename_entry,
            delete_entry,
            query_calendar_days,
            query_calendar_date_notes,
            get_backlinks,
            get_all_tags,
            get_notes_by_tag,
            search_vault,
            rebuild_vault_index,
            save_pasted_image,
            read_vault_image_data_url,
            export_active_note_html,
            render_note_html_preview,
            query_due_tasks,
            get_note_cards,
            get_vault_health,
            plan_link_updates,
            apply_link_edits,
            pick_task_calendar_path,
            load_app_settings,
            save_app_settings,
            export_settings_file,
            import_settings_file,
            write_task_calendar,
            caldav_set_password,
            caldav_has_password,
            caldav_forget,
            caldav_sync,
            configure_task_reminders,
            configure_backups,
            get_backup_status,
            run_backup_now,
            pick_backup_folder,
            list_pinned_notes,
            set_note_pinned,
            list_note_versions,
            read_note_version,
            restore_note_version,
            configure_version_history,
            list_deleted_entries,
            restore_deleted_entry,
            list_css_snippets,
            open_snippets_folder,
            list_spell_dictionaries,
            spell_check_words,
            spell_suggest,
            spell_add_word,
            open_dictionaries_folder,
            configure_summon,
            configure_quick_capture,
            quick_capture_append,
            hide_capture_window,
            configure_window_behavior,
            quit_app,
            keep_app_open,
            set_tray_labels,
            ai_get_status,
            ai_save_config,
            ai_save_key,
            ai_delete_key,
            ai_run,
            ask_index_status,
            ask_index_vault,
            ask_vault,
            ask_delete_index,
            ai_cancel,
            get_cli_path,
            check_for_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running Jotter Tauri application");
}

/// What the main window remembers between launches: its size, its position, and whether it was
/// maximized. Visibility is left out so a window hidden in the tray when Jotter quit does not
/// make the next launch start hidden. Fullscreen is left out because zen focus mode turns it on
/// and off, and a launch should not start in a mode the user entered for one session.
#[cfg(desktop)]
const WINDOW_STATE_FLAGS: tauri_plugin_window_state::StateFlags =
    tauri_plugin_window_state::StateFlags::SIZE
        .union(tauri_plugin_window_state::StateFlags::POSITION)
        .union(tauri_plugin_window_state::StateFlags::MAXIMIZED);

/// Windows that are not remembered: the quick capture window sizes and places itself.
#[cfg(desktop)]
const WINDOW_STATE_SKIPPED: &[&str] = &["capture"];

/// Saves the main window's geometry when Jotter closes and restores it at the next launch. The
/// plugin ignores a saved position that no connected monitor shows, so unplugging a screen does
/// not strand the window off-screen.
#[cfg(desktop)]
fn window_state_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_window_state::Builder::default()
        .with_state_flags(WINDOW_STATE_FLAGS)
        .with_denylist(WINDOW_STATE_SKIPPED)
        .build()
}

#[cfg(all(test, desktop))]
mod window_state_tests {
    use super::*;
    use tauri_plugin_window_state::StateFlags;

    #[test]
    fn the_main_window_keeps_geometry_but_not_visibility_or_fullscreen() {
        assert!(WINDOW_STATE_FLAGS.contains(StateFlags::SIZE));
        assert!(WINDOW_STATE_FLAGS.contains(StateFlags::POSITION));
        assert!(WINDOW_STATE_FLAGS.contains(StateFlags::MAXIMIZED));
        assert!(!WINDOW_STATE_FLAGS.contains(StateFlags::VISIBLE));
        assert!(!WINDOW_STATE_FLAGS.contains(StateFlags::FULLSCREEN));
        assert!(!WINDOW_STATE_FLAGS.contains(StateFlags::DECORATIONS));
    }

    #[test]
    fn the_capture_window_is_not_remembered() {
        assert!(WINDOW_STATE_SKIPPED.contains(&"capture"));
        assert!(!WINDOW_STATE_SKIPPED.contains(&"main"));
    }
}

#[cfg(test)]
mod vault_index_path_tests {
    use super::*;

    #[test]
    fn each_vault_gets_its_own_index_database() {
        let data = Path::new("C:/Users/test/AppData/Local/Jotter");
        let a = vault_index_db_path(data, Path::new("C:/Notes/Work"));
        let b = vault_index_db_path(data, Path::new("C:/Notes/Personal"));
        assert_ne!(a, b);
        assert!(a.starts_with(data.join("indexes")));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn same_vault_maps_to_the_same_database_regardless_of_case_and_slashes() {
        let data = Path::new("C:/Users/test/AppData/Local/Jotter");
        let a = vault_index_db_path(data, Path::new(r"C:\Notes\Work"));
        let b = vault_index_db_path(data, Path::new("c:/notes/work/"));
        assert_eq!(a, b);
    }
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn case_sensitive_vaults_do_not_share_an_index() {
        let data = Path::new("/tmp/Jotter");
        assert_ne!(vault_index_db_path(data, Path::new("/notes/Work")),
                   vault_index_db_path(data, Path::new("/notes/work")));
    }

}

#[cfg(test)]
mod command_permission_tests {
    use std::collections::BTreeSet;

    /// The quoted names in `build.rs`'s `COMMANDS` list.
    fn manifest_commands() -> BTreeSet<String> {
        let source = include_str!("../build.rs");
        let list = source.split("= &[").nth(1).unwrap().split("];").next().unwrap();
        list.split('"').skip(1).step_by(2).map(str::to_string).collect()
    }

    /// The names inside `generate_handler![…]` in this file.
    fn handler_commands() -> BTreeSet<String> {
        let source = include_str!("lib.rs");
        let start = source.find(concat!("generate_handler", "![")).unwrap();
        let list = source[start..].split('[').nth(1).unwrap().split(']').next().unwrap();
        list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
    }

    fn capability_permissions(json: &str) -> BTreeSet<String> {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        value["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str())
            .filter(|p| p.starts_with("allow-"))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn every_command_is_listed_and_allowed_for_the_main_window() {
        let handlers = handler_commands();
        assert_eq!(manifest_commands(), handlers, "build.rs COMMANDS must match generate_handler");
        let allowed: BTreeSet<String> =
            handlers.iter().map(|c| format!("allow-{}", c.replace('_', "-"))).collect();
        assert_eq!(capability_permissions(include_str!("../capabilities/default.json")), allowed);
    }

    #[test]
    fn the_capture_window_reaches_only_what_it_needs() {
        let expected: BTreeSet<String> =
            ["allow-hide-capture-window", "allow-load-app-settings"]
                .map(str::to_string)
                .into();
        assert_eq!(capability_permissions(include_str!("../capabilities/capture.json")), expected);
    }
}
