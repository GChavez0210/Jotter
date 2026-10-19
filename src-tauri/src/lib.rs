pub mod clipboard;
pub mod fs_atomic;
pub mod index;
pub mod links;
pub mod search;
pub mod types;
pub mod vault;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::clipboard::{
    build_clipboard_payload, read_plain_from_system_clipboard, write_to_system_clipboard,
};
use crate::index::{
    apply_scanned_vault_index, delete_indexed_path, init_index_schema, query_all_tags,
    query_backlinks_for_note, query_calendar_month, query_notes_by_tag, query_notes_on_date,
    rename_edit_log_paths, scan_dir_files_from_disk, scan_single_file_from_disk,
    system_time_to_ms, upsert_scanned_note, ScannedNoteFile,
};
use crate::search::VaultSearchIndex;
use crate::types::{
    BackdropState, BacklinkEntry, CalendarDateField, CalendarDaySummary, CalendarNoteEntry,
    ClipboardPayload, CopyRequest, NoteDocument, RecentVault, RenameEntryResult, SearchHit,
    TagSummary, VaultFsEvent, VaultSnapshot,
};
use crate::vault::{
    create_vault_folder, create_vault_note, delete_vault_entry, load_recent_vaults,
    read_vault_note, record_recent_vault, remove_recent_vault, rename_vault_entry,
    save_vault_note, scan_vault_snapshot, validate_windows_entry_name,
};

pub struct AppState {
    pub backdrop: Mutex<BackdropState>,
    pub active_vault: Mutex<Option<PathBuf>>,
    pub sqlite_conn: Arc<Mutex<Connection>>,
    pub search_index: Arc<VaultSearchIndex>,
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    pub recent_self_writes: Arc<Mutex<HashMap<String, Instant>>>,
    pub recent_vaults_file: PathBuf,
}

/// Applies the Windows 11 Mica backdrop via `window-vibrancy`, returning whether Mica succeeded
/// or if the frontend should use the solid-color fallback (e.g. on Windows 10).
fn try_apply_mica(window: &tauri::WebviewWindow, dark_mode: Option<bool>) -> BackdropState {
    #[cfg(target_os = "windows")]
    {
        match window_vibrancy::apply_mica(window, dark_mode) {
            Ok(()) => BackdropState {
                mica_applied: true,
                backdrop_kind: "mica".to_string(),
                dark_mode,
            },
            Err(_) => BackdropState {
                mica_applied: false,
                backdrop_kind: "solid_fallback".to_string(),
                dark_mode,
            },
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        BackdropState {
            mica_applied: false,
            backdrop_kind: "solid_fallback".to_string(),
            dark_mode,
        }
    }
}

fn require_active_vault(state: &State<'_, AppState>) -> Result<PathBuf, String> {
    let guard = state
        .active_vault
        .lock()
        .map_err(|e| format!("Failed to lock active vault state: {e}"))?;
    guard
        .clone()
        .ok_or_else(|| "No vault is currently open".to_string())
}

fn mark_self_write(
    recent_self_writes: &Arc<Mutex<HashMap<String, Instant>>>,
    rel_path: &str,
) {
    if let Ok(mut map) = recent_self_writes.lock() {
        let now = Instant::now();
        map.retain(|_, ts| now.duration_since(*ts) < Duration::from_secs(3));
        map.insert(rel_path.to_lowercase(), now);
    }
}

fn is_recent_self_write(
    recent_self_writes: &Arc<Mutex<HashMap<String, Instant>>>,
    rel_path: &str,
) -> bool {
    if let Ok(mut map) = recent_self_writes.lock() {
        let now = Instant::now();
        map.retain(|_, ts| now.duration_since(*ts) < Duration::from_secs(3));
        if let Some(ts) = map.get(&rel_path.to_lowercase()) {
            return now.duration_since(*ts) < Duration::from_millis(900);
        }
    }
    false
}

/// Processes a debounced batch of changed relative paths incrementally:
/// walks/reads any changed files or folders outside the SQLite/Tantivy locks, then updates
/// SQLite (`notes`, `links`, `note_tags`) and Tantivy incrementally.
fn process_watcher_batch(
    vault_root: &Path,
    sqlite_conn: &Arc<Mutex<Connection>>,
    search_index: &Arc<VaultSearchIndex>,
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

    // 2. Apply incremental SQLite updates
    if let Ok(conn) = sqlite_conn.lock() {
        for op in &ops {
            match op {
                IncrementalOp::Delete(rel) => {
                    let _ = delete_indexed_path(&conn, rel);
                }
                IncrementalOp::SingleFile(scanned) => {
                    let _ = upsert_scanned_note(&conn, scanned, None);
                }
                IncrementalOp::DirFiles(dir_rel, scanned_list) => {
                    let _ = delete_indexed_path(&conn, dir_rel);
                    for scanned in scanned_list {
                        let _ = upsert_scanned_note(&conn, scanned, None);
                    }
                }
            }
        }
    }

    // 3. Apply incremental Tantivy full-text search updates
    for op in &ops {
        match op {
            IncrementalOp::Delete(rel) => {
                let _ = search_index.delete_path_or_prefix(rel);
            }
            IncrementalOp::SingleFile(scanned) => {
                let _ = search_index.upsert_scanned_note(scanned);
            }
            IncrementalOp::DirFiles(dir_rel, scanned_list) => {
                let _ = search_index.delete_path_or_prefix(dir_rel);
                let _ = search_index.upsert_batch(scanned_list);
            }
        }
    }
}

fn start_vault_watcher(
    app_handle: AppHandle,
    vault_root: PathBuf,
    sqlite_conn: Arc<Mutex<Connection>>,
    search_index: Arc<VaultSearchIndex>,
    recent_self_writes: Arc<Mutex<HashMap<String, Instant>>>,
) -> Result<RecommendedWatcher, String> {
    let (tx, rx) = mpsc::channel::<Vec<String>>();
    let root_for_cb = vault_root.clone();

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
                let Ok(rel) = path.strip_prefix(&root_for_cb) else {
                    continue;
                };
                // Ignore .obsidian, .git, and temporary atomic write dot-files
                if rel.components().any(|c| {
                    c.as_os_str()
                        .to_str()
                        .is_some_and(|s| s.starts_with('.'))
                }) {
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
                if is_recent_self_write(&recent_self_writes, &rel_str) {
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
        .map_err(|e| format!("Failed to watch vault directory {}: {e}", vault_root.display()))?;

    // Background debounce worker (300 ms quiet window, batched)
    let root_for_worker = vault_root;
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
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        process_watcher_batch(
                            &root_for_worker,
                            &sqlite_conn,
                            &search_index,
                            &batched,
                        );
                        let _ = app_handle.emit(
                            "vault-fs-event",
                            VaultFsEvent {
                                changed_paths: batched,
                            },
                        );
                        return;
                    }
                }
            }

            process_watcher_batch(&root_for_worker, &sqlite_conn, &search_index, &batched);
            let _ = app_handle.emit(
                "vault-fs-event",
                VaultFsEvent {
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

#[tauri::command]
fn sync_window_theme(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    dark_mode: Option<bool>,
) -> Result<BackdropState, String> {
    let next_state = try_apply_mica(&window, dark_mode);
    if let Ok(mut guard) = state.backdrop.lock() {
        *guard = next_state.clone();
    }
    Ok(next_state)
}

#[tauri::command]
fn get_backdrop_state(state: State<'_, AppState>) -> Result<BackdropState, String> {
    state
        .backdrop
        .lock()
        .map(|g| g.clone())
        .map_err(|e| format!("Failed to read backdrop state: {e}"))
}

#[tauri::command]
fn pick_vault_folder(title: Option<String>) -> Result<Option<String>, String> {
    let prompt = title.unwrap_or_else(|| "Select an Obsidian / Markdown vault folder".to_string());
    #[cfg(target_os = "windows")]
    {
        Ok(crate::vault::pick_folder_dialog_win32(&prompt)
            .map(|p| p.to_string_lossy().into_owned()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = prompt;
        Ok(None)
    }
}

#[tauri::command]
fn open_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<VaultSnapshot, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Vault path cannot be empty".to_string());
    }
    let vault_root = PathBuf::from(trimmed);
    let snapshot = scan_vault_snapshot(&vault_root)?;

    // Walk the filesystem and parse notes/links/tags BEFORE locking SQLite or Tantivy
    let scanned_files = scan_dir_files_from_disk(&vault_root, &vault_root)?;
    {
        let conn = state
            .sqlite_conn
            .lock()
            .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
        apply_scanned_vault_index(&conn, &scanned_files)?;
    }
    state.search_index.rebuild_from_scanned(&scanned_files)?;

    let watcher = start_vault_watcher(
        app_handle,
        vault_root.clone(),
        Arc::clone(&state.sqlite_conn),
        Arc::clone(&state.search_index),
        Arc::clone(&state.recent_self_writes),
    )?;

    if let Ok(mut w_guard) = state.watcher.lock() {
        *w_guard = Some(watcher);
    }
    if let Ok(mut v_guard) = state.active_vault.lock() {
        *v_guard = Some(vault_root.clone());
    }

    let _ = record_recent_vault(&state.recent_vaults_file, &vault_root);
    Ok(snapshot)
}

#[tauri::command]
fn create_new_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    parent_path: String,
    vault_name: String,
) -> Result<VaultSnapshot, String> {
    validate_windows_entry_name(&vault_name)?;
    let clean_name = vault_name.trim_start();

    let parent = PathBuf::from(parent_path.trim());
    if !parent.exists() || !parent.is_dir() {
        return Err(format!(
            "Parent directory does not exist: {}",
            parent.display()
        ));
    }

    let vault_root = parent.join(clean_name);
    std::fs::create_dir_all(&vault_root)
        .map_err(|e| format!("Failed to create vault directory {}: {e}", vault_root.display()))?;

    // Seed a starter Welcome.md note only if the newly created folder is completely empty
    let welcome_path = vault_root.join("Welcome.md");
    if !welcome_path.exists() {
        let initial_welcome = format!(
            "# Welcome to {clean_name}\n\nThis vault is stored as plain `.md` files on disk.\n\n- Press **Ctrl+N** to create a new note\n- Press **Ctrl+C** to copy with Notepad-identical line breaks\n- Press **Ctrl+Shift+V** to paste as plain text\n"
        );
        let _ = crate::fs_atomic::atomic_write_str(&welcome_path, &initial_welcome);
    }

    open_vault(
        app_handle,
        state,
        vault_root.to_string_lossy().into_owned(),
    )
}

#[tauri::command]
fn get_vault_snapshot(state: State<'_, AppState>) -> Result<Option<VaultSnapshot>, String> {
    let root_opt = state
        .active_vault
        .lock()
        .map_err(|e| format!("Failed to lock active vault: {e}"))?
        .clone();
    match root_opt {
        Some(root) => Ok(Some(scan_vault_snapshot(&root)?)),
        None => Ok(None),
    }
}

#[tauri::command]
fn close_vault(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(mut w_guard) = state.watcher.lock() {
        *w_guard = None;
    }
    if let Ok(mut v_guard) = state.active_vault.lock() {
        *v_guard = None;
    }
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

#[tauri::command]
fn read_note(state: State<'_, AppState>, rel_path: String) -> Result<NoteDocument, String> {
    let vault_root = require_active_vault(&state)?;
    read_vault_note(&vault_root, &rel_path)
}

#[tauri::command]
fn create_note(
    state: State<'_, AppState>,
    rel_path: String,
    initial_content: Option<String>,
) -> Result<(VaultSnapshot, NoteDocument), String> {
    let vault_root = require_active_vault(&state)?;
    let content = initial_content.unwrap_or_default();
    let (snapshot, doc) = create_vault_note(&vault_root, &rel_path, &content)?;
    mark_self_write(&state.recent_self_writes, &doc.rel_path);

    let abs = vault_root.join(&doc.rel_path);
    let now_ms = system_time_to_ms(SystemTime::now());
    let scanned = scan_single_file_from_disk(&vault_root, &abs)?;
    {
        let conn = state
            .sqlite_conn
            .lock()
            .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
        upsert_scanned_note(&conn, &scanned, Some(now_ms))?;
    }
    state.search_index.upsert_scanned_note(&scanned)?;

    Ok((snapshot, doc))
}

#[tauri::command]
fn save_note(
    state: State<'_, AppState>,
    rel_path: String,
    content: String,
) -> Result<NoteDocument, String> {
    let vault_root = require_active_vault(&state)?;
    let normalized_rel = crate::vault::ensure_md_extension(&rel_path);
    mark_self_write(&state.recent_self_writes, &normalized_rel);

    let (doc, changed) = save_vault_note(&vault_root, &normalized_rel, &content)?;
    if changed {
        mark_self_write(&state.recent_self_writes, &doc.rel_path);
        let abs = vault_root.join(&doc.rel_path);
        let now_ms = system_time_to_ms(SystemTime::now());
        let scanned = scan_single_file_from_disk(&vault_root, &abs)?;
        {
            let conn = state
                .sqlite_conn
                .lock()
                .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
            upsert_scanned_note(&conn, &scanned, Some(now_ms))?;
        }
        state.search_index.upsert_scanned_note(&scanned)?;
    }

    Ok(doc)
}

#[tauri::command]
fn create_folder(state: State<'_, AppState>, rel_path: String) -> Result<VaultSnapshot, String> {
    let vault_root = require_active_vault(&state)?;
    let clean = crate::vault::normalize_rel_path(&rel_path);
    mark_self_write(&state.recent_self_writes, &clean);
    create_vault_folder(&vault_root, &clean)
}

#[tauri::command]
fn rename_entry(
    state: State<'_, AppState>,
    old_rel_path: String,
    new_rel_path: String,
) -> Result<RenameEntryResult, String> {
    let vault_root = require_active_vault(&state)?;
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

    {
        let conn = state
            .sqlite_conn
            .lock()
            .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
        rename_edit_log_paths(&conn, &res.old_rel_path, &res.new_rel_path)?;
        delete_indexed_path(&conn, &res.old_rel_path)?;
        if let Some(scanned) = &single_scanned {
            upsert_scanned_note(&conn, scanned, None)?;
        } else if let Some(scanned_list) = &dir_scanned {
            for scanned in scanned_list {
                upsert_scanned_note(&conn, scanned, None)?;
            }
        }
    }

    state.search_index.delete_path_or_prefix(&res.old_rel_path)?;
    if let Some(scanned) = &single_scanned {
        state.search_index.upsert_scanned_note(scanned)?;
    } else if let Some(scanned_list) = &dir_scanned {
        state.search_index.upsert_batch(scanned_list)?;
    }

    Ok(res)
}

#[tauri::command]
fn delete_entry(state: State<'_, AppState>, rel_path: String) -> Result<VaultSnapshot, String> {
    let vault_root = require_active_vault(&state)?;
    let clean = crate::vault::normalize_rel_path(&rel_path);
    mark_self_write(&state.recent_self_writes, &clean);

    let snapshot = delete_vault_entry(&vault_root, &clean)?;
    {
        let conn = state
            .sqlite_conn
            .lock()
            .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
        delete_indexed_path(&conn, &clean)?;
    }
    state.search_index.delete_path_or_prefix(&clean)?;

    Ok(snapshot)
}

#[tauri::command]
fn query_calendar_days(
    state: State<'_, AppState>,
    year_month: String,
    field: CalendarDateField,
) -> Result<Vec<CalendarDaySummary>, String> {
    let conn = state
        .sqlite_conn
        .lock()
        .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
    query_calendar_month(&conn, &year_month, field)
}

#[tauri::command]
fn query_calendar_date_notes(
    state: State<'_, AppState>,
    date: String,
    field: CalendarDateField,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock()
        .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
    query_notes_on_date(&conn, &date, field)
}

#[tauri::command]
fn get_backlinks(
    state: State<'_, AppState>,
    rel_path: String,
) -> Result<Vec<BacklinkEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock()
        .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
    query_backlinks_for_note(&conn, &rel_path)
}

#[tauri::command]
fn get_all_tags(state: State<'_, AppState>) -> Result<Vec<TagSummary>, String> {
    let conn = state
        .sqlite_conn
        .lock()
        .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
    query_all_tags(&conn)
}

#[tauri::command]
fn get_notes_by_tag(
    state: State<'_, AppState>,
    tag: String,
) -> Result<Vec<CalendarNoteEntry>, String> {
    let conn = state
        .sqlite_conn
        .lock()
        .map_err(|e| format!("Failed to lock SQLite index: {e}"))?;
    query_notes_by_tag(&conn, &tag)
}

#[tauri::command]
fn search_vault(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let vault_root = state
        .active_vault
        .lock()
        .map_err(|e| format!("Failed to lock active vault state: {e}"))?
        .clone();
    state
        .search_index
        .search(vault_root.as_deref(), &query, limit.unwrap_or(40))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Jotter");
    let _ = std::fs::create_dir_all(&app_data_dir);
    let recent_vaults_file = app_data_dir.join("recent_vaults.json");
    let db_path = app_data_dir.join("vault_index_cache.sqlite");

    let conn = Connection::open(&db_path)
        .or_else(|_| Connection::open_in_memory())
        .expect("Failed to open SQLite index cache");
    let _ = init_index_schema(&conn);

    let search_idx = VaultSearchIndex::new().expect("Failed to initialize Tantivy search index");

    tauri::Builder::default()
        .manage(AppState {
            backdrop: Mutex::new(BackdropState {
                mica_applied: false,
                backdrop_kind: "initializing".to_string(),
                dark_mode: None,
            }),
            active_vault: Mutex::new(None),
            sqlite_conn: Arc::new(Mutex::new(conn)),
            search_index: Arc::new(search_idx),
            watcher: Mutex::new(None),
            recent_self_writes: Arc::new(Mutex::new(HashMap::new())),
            recent_vaults_file,
        })
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let initial_dark = window
                    .theme()
                    .ok()
                    .map(|t| matches!(t, tauri::Theme::Dark));
                let applied = try_apply_mica(&window, initial_dark);
                let state = app.state::<AppState>();
                if let Ok(mut guard) = state.backdrop.lock() {
                    *guard = applied;
                };
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            copy_to_clipboard,
            preview_clipboard_payload,
            paste_plain_from_clipboard,
            sync_window_theme,
            get_backdrop_state,
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
            search_vault
        ])
        .run(tauri::generate_context!())
        .expect("error while running Jotter Tauri application");
}
