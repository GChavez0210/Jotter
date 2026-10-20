//! Summon hotkey and tray icon.
//!
//! When the summon hotkey is on, Jotter keeps a tray icon, closing the window hides it instead
//! of quitting, and the hotkey brings the window to the front from any app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TRAY_ID: &str = "jotter-tray";

/// Event the frontend handles by saving open notes and then calling `quit_app`.
pub const QUIT_REQUESTED_EVENT: &str = "quit-requested";

#[derive(Default)]
pub struct SummonState {
    /// Closing the main window hides it while this is set.
    pub resident: AtomicBool,
    pub shortcut: Mutex<Option<Shortcut>>,
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the window, or hides it when it is already in front, so one hotkey toggles Jotter.
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let in_front = window.is_visible().unwrap_or(false)
        && window.is_focused().unwrap_or(false)
        && !window.is_minimized().unwrap_or(false);
    if in_front {
        let _ = window.hide();
    } else {
        show_main_window(app);
    }
}

/// Exiting or hiding requires the frontend's successful save acknowledgement.
pub fn request_close(app: &AppHandle, quit: bool) {
    if app.emit(QUIT_REQUESTED_EVENT, quit).is_err() {
        show_main_window(app);
    }
}

/// Builds the tray icon, hidden until the summon hotkey is turned on.
pub fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Jotter", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Jotter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Jotter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => request_close(app, true),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    tray.set_visible(false)?;
    Ok(())
}

/// Turns the summon hotkey and tray residency on or off. Replaces any previously registered
/// hotkey. Returns an error when the shortcut text is invalid or another app already owns it.
pub fn configure(app: &AppHandle, enabled: bool, shortcut_text: &str) -> Result<(), String> {
    let state = app.state::<SummonState>();
    let global = app.global_shortcut();

    let mut current = state
        .shortcut
        .lock()
        .map_err(|e| format!("Failed to lock shortcut state: {e}"))?;
    let next = if enabled {
        Some(shortcut_text.parse::<Shortcut>()
            .map_err(|e| format!("'{shortcut_text}' is not a valid shortcut: {e}"))?)
    } else { None };
    replace_shortcut(&mut current, next,
        |shortcut| global.on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed { toggle_main_window(app); }
        }).map_err(|e| format!("Cannot register {shortcut_text}. Another app may already use it: {e}")),
        |shortcut| global.unregister(shortcut).map_err(|e| format!("Cannot unregister shortcut: {e}")),
    )?;

    state.resident.store(enabled, Ordering::SeqCst);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(enabled);
    }
    Ok(())
}


/// Keep the previous hotkey and residency when a replacement cannot be registered.
fn replace_shortcut<T: Copy + Eq>(
    current: &mut Option<T>, next: Option<T>,
    mut register: impl FnMut(T) -> Result<(), String>,
    mut unregister: impl FnMut(T) -> Result<(), String>,
) -> Result<(), String> {
    if *current == next { return Ok(()); }
    if let Some(candidate) = next { register(candidate)?; }
    if let Some(previous) = *current {
        if let Err(err) = unregister(previous) {
            if let Some(candidate) = next { let _ = unregister(candidate); }
            return Err(err);
        }
    }
    *current = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::replace_shortcut;
    use std::cell::RefCell;

    #[test]
    fn failed_replacement_keeps_previous_hotkey() {
        let mut current = Some(1);
        let removed = RefCell::new(Vec::new());
        assert!(replace_shortcut(&mut current, Some(2), |_| Err("occupied".into()),
            |id| { removed.borrow_mut().push(id); Ok(()) }).is_err());
        assert_eq!(current, Some(1));
        assert!(removed.borrow().is_empty());
    }

    #[test]
    fn successful_replacement_registers_before_removing_previous() {
        let mut current = Some(1);
        let events = RefCell::new(Vec::new());
        replace_shortcut(&mut current, Some(2),
            |id| { events.borrow_mut().push(("register", id)); Ok(()) },
            |id| { events.borrow_mut().push(("unregister", id)); Ok(()) }).unwrap();
        assert_eq!(current, Some(2));
        assert_eq!(*events.borrow(), vec![("register", 2), ("unregister", 1)]);
    }

    #[test]
    fn failed_unregister_rolls_back_candidate() {
        let mut current = Some(1);
        let removed = RefCell::new(Vec::new());
        assert!(replace_shortcut(&mut current, Some(2), |_| Ok(()), |id| {
            removed.borrow_mut().push(id);
            if id == 1 { Err("failed".into()) } else { Ok(()) }
        }).is_err());
        assert_eq!(current, Some(1));
        assert_eq!(*removed.borrow(), vec![1, 2]);
    }
}
