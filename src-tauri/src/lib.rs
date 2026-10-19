pub mod clipboard;
pub mod fs_atomic;
pub mod index;
pub mod links;
pub mod types;

use std::sync::Mutex;
use tauri::{Manager, State};

use crate::clipboard::{
    build_clipboard_payload, read_plain_from_system_clipboard, write_to_system_clipboard,
};
use crate::types::{BackdropState, ClipboardPayload, CopyRequest};

pub struct AppState {
    pub backdrop: Mutex<BackdropState>,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            backdrop: Mutex::new(BackdropState {
                mica_applied: false,
                backdrop_kind: "initializing".to_string(),
                dark_mode: None,
            }),
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
            get_backdrop_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running Jotter Tauri application");
}
