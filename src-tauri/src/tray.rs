//! Summon hotkey, tray icon, and close-button behavior.
//!
//! Three settings work independently: the summon hotkey brings the window to the front from any
//! app, tray residency keeps a tray icon, and the close button either quits Jotter or hides the
//! window. The tray menu's Quit always exits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const TRAY_ID: &str = "jotter-tray";

/// Event the frontend handles by saving open notes and then calling `quit_app`.
pub const QUIT_REQUESTED_EVENT: &str = "quit-requested";

#[derive(Default)]
pub struct BackgroundState {
    /// The tray icon is visible while this is set.
    pub tray_resident: AtomicBool,
    /// Closing the main window hides it while this is set.
    pub hide_on_close: AtomicBool,
    pub shortcut: Mutex<Option<Shortcut>>,
    /// The quick capture hotkey, kept apart from the summon hotkey.
    pub capture_shortcut: Mutex<Option<Shortcut>>,
}

/// Label of the small window quick capture opens.
pub const CAPTURE_LABEL: &str = "capture";

/// Event the capture window handles by focusing its text box.
pub const CAPTURE_OPEN_EVENT: &str = "quick-capture-open";

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

/// Shows the capture window, creating it on first use, or hides it when it is already in front.
fn toggle_capture_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(CAPTURE_LABEL) {
        let in_front = window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false);
        if in_front {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.emit(CAPTURE_OPEN_EVENT, ());
        }
        return;
    }
    // Windows deadlocks when a window is built on the event loop thread, which runs hotkeys.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, CAPTURE_LABEL, WebviewUrl::App("index.html".into()))
            // The window sets its localized title once its page loads.
            .title("Jotter")
            .inner_size(520.0, 200.0)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .center()
            .focused(true)
            .build();
        if let Err(err) = built {
            eprintln!("Cannot open the quick capture window: {err}");
        }
    });
}

/// Hides the capture window. The window stays alive so the next hotkey press opens it at once.
pub fn hide_capture_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(CAPTURE_LABEL) {
        let _ = window.hide();
    }
}

/// Exiting or hiding requires the frontend's successful save acknowledgement.
pub fn request_close(app: &AppHandle, quit: bool) {
    if app.emit(QUIT_REQUESTED_EVENT, quit).is_err() {
        show_main_window(app);
    }
}

fn tray_menu<R: tauri::Runtime, M: Manager<R>>(app: &M, show: &str, quit: &str) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", show, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit, true, None::<&str>)?;
    Menu::with_items(app, &[&show, &quit])
}

/// Replaces the tray menu labels. The menu item ids, and so their handlers, stay the same.
pub fn set_labels(app: &AppHandle, show: &str, quit: &str) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(tray_menu(app, show, quit)?))?;
    }
    Ok(())
}

/// Builds the tray icon, hidden until the summon hotkey is turned on.
pub fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = tray_menu(app, "Show Jotter", "Quit Jotter")?;

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

/// Turns the summon hotkey on or off. Replaces any previously registered hotkey. Returns an
/// error when the shortcut text is invalid or another app already owns it.
pub fn configure(app: &AppHandle, enabled: bool, shortcut_text: &str) -> Result<(), String> {
    let state = app.state::<BackgroundState>();
    configure_slot(app, &state.shortcut, enabled, shortcut_text, toggle_main_window)
}

/// Turns the quick capture hotkey on or off, with the same rules as the summon hotkey.
pub fn configure_capture(app: &AppHandle, enabled: bool, shortcut_text: &str) -> Result<(), String> {
    let state = app.state::<BackgroundState>();
    configure_slot(app, &state.capture_shortcut, enabled, shortcut_text, toggle_capture_window)
}

fn configure_slot(
    app: &AppHandle,
    slot: &Mutex<Option<Shortcut>>,
    enabled: bool,
    shortcut_text: &str,
    on_press: fn(&AppHandle),
) -> Result<(), String> {
    let global = app.global_shortcut();

    let mut current = slot
        .lock()
        .map_err(|e| format!("Failed to lock shortcut state: {e}"))?;
    let next = if enabled {
        Some(shortcut_text.parse::<Shortcut>()
            .map_err(|e| format!("'{shortcut_text}' is not a valid shortcut: {e}"))?)
    } else { None };
    replace_shortcut(&mut current, next,
        |shortcut| global.on_shortcut(shortcut, move |app, _, event| {
            if event.state == ShortcutState::Pressed { on_press(app); }
        }).map_err(|e| format!("Cannot register {shortcut_text}. Another app may already use it: {e}")),
        |shortcut| global.unregister(shortcut).map_err(|e| format!("Cannot unregister shortcut: {e}")),
    )?;
    Ok(())
}

/// Shows or hides the tray icon and records what the window's close button does.
pub fn set_window_behavior(
    app: &AppHandle,
    tray_resident: bool,
    hide_on_close: bool,
) -> Result<(), String> {
    let state = app.state::<BackgroundState>();
    // Move the icon first. A failed change leaves the previous settings in place, so the window
    // never hides into a tray that is not there.
    let tray = app.tray_by_id(TRAY_ID);
    if let Some(tray) = &tray {
        tray.set_visible(tray_resident)
            .map_err(|e| format!("Cannot change the tray icon: {e}"))?;
    }
    state
        .tray_resident
        .store(tray_resident && tray.is_some(), Ordering::SeqCst);
    state.hide_on_close.store(hide_on_close, Ordering::SeqCst);
    Ok(())
}

/// Hiding the window is safe only when the tray icon or the summon hotkey can bring it back.
pub fn hides_on_close(hide_on_close: bool, tray_resident: bool, summon_registered: bool) -> bool {
    hide_on_close && (tray_resident || summon_registered)
}

/// Answers whether closing the window hides it, from the current settings.
pub fn should_hide_window(app: &AppHandle) -> bool {
    let state = app.state::<BackgroundState>();
    let summon_registered = state
        .shortcut
        .lock()
        .map(|shortcut| shortcut.is_some())
        .unwrap_or(false);
    hides_on_close(
        state.hide_on_close.load(Ordering::SeqCst),
        state.tray_resident.load(Ordering::SeqCst),
        summon_registered,
    )
}

/// Keep the previous hotkey when a replacement cannot be registered.
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
    use super::{hides_on_close, replace_shortcut};
    use std::cell::RefCell;

    #[test]
    fn close_button_quits_unless_the_user_chose_to_hide() {
        assert!(!hides_on_close(false, true, true));
        assert!(hides_on_close(true, true, false));
        assert!(hides_on_close(true, false, true));
    }

    #[test]
    fn hiding_without_a_way_back_quits_instead() {
        assert!(!hides_on_close(true, false, false));
    }

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
