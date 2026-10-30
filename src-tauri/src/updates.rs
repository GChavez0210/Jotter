//! Update checks and installs.
//!
//! The page never talks to the update feed. It asks these commands, which call
//! `tauri-plugin-updater`, so the page's content security policy stays closed to the network
//! and the plugin needs no window permission. The plugin verifies every download against the
//! public key in `tauri.conf.json` before it installs anything.

use serde::Serialize;

pub const PROGRESS_EVENT: &str = "update-progress";

/// What the page shows about an available update.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// The release notes from the feed, in Markdown.
    pub notes: Option<String>,
    /// The release date as `YYYY-MM-DD`.
    pub date: Option<String>,
    /// False when the update has to come from the release page instead.
    pub can_install: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum UpdateProgress {
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
}

/// A Linux install replaces the AppImage in place. A `.deb` belongs to the package manager,
/// so it cannot update itself. Windows and macOS installs always can.
pub fn can_self_install(is_linux: bool, running_from_appimage: bool) -> bool {
    !is_linux || running_from_appimage
}

fn can_self_install_here() -> bool {
    can_self_install(cfg!(target_os = "linux"), std::env::var_os("APPIMAGE").is_some())
}

#[cfg(desktop)]
mod desktop {
    use std::sync::Mutex;

    use tauri::{AppHandle, Emitter, State};
    use tauri_plugin_updater::{Update, UpdaterExt};

    use super::{can_self_install_here, UpdateInfo, UpdateProgress, PROGRESS_EVENT};

    /// The update the last check found, kept so the install downloads exactly what the page showed.
    #[derive(Default)]
    pub struct UpdateState {
        pending: Mutex<Option<Update>>,
        // A check must not replace the offer while an install owns it.
        operation: tokio::sync::Mutex<()>,
    }

    impl UpdateState {
        fn set(&self, update: Option<Update>) {
            *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = update;
        }

        fn get(&self) -> Option<Update> {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    fn describe(update: &Update) -> UpdateInfo {
        UpdateInfo {
            version: update.version.clone(),
            current_version: update.current_version.clone(),
            notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
            date: update.date.map(|date| date.date().to_string()),
            can_install: can_self_install_here(),
        }
    }

    pub async fn check(app: AppHandle, state: State<'_, UpdateState>) -> Result<Option<UpdateInfo>, String> {
        let _operation = state.operation.try_lock().map_err(|_| "update_busy".to_string())?;
        let updater = app.updater().map_err(|_| "update_check_failed".to_string())?;
        let found = updater.check().await.map_err(|_| "update_check_failed".to_string())?;
        let info = found.as_ref().map(describe);
        state.set(found);
        Ok(info)
    }

    pub async fn install(app: AppHandle, state: State<'_, UpdateState>, expected_version: String) -> Result<(), String> {
        let _operation = state.operation.try_lock().map_err(|_| "update_busy".to_string())?;
        if !can_self_install_here() {
            return Err("update_cannot_install".into());
        }
        let update = state.get().filter(|update| update.version == expected_version)
            .ok_or("update_offer_changed")?;
        let mut downloaded = 0u64;
        let progress = app.clone();
        let finished = app.clone();
        update
            .download_and_install(
                move |chunk, total| {
                    downloaded += chunk as u64;
                    let _ = progress.emit(PROGRESS_EVENT, UpdateProgress::Downloading { downloaded, total });
                },
                move || {
                    let _ = finished.emit(PROGRESS_EVENT, UpdateProgress::Installing);
                },
            )
            .await
            .map_err(|_| "update_install_failed".to_string())?;
        // The Windows installer ends this process itself. Other systems restart here.
        app.restart()
    }
}

#[cfg(desktop)]
pub use desktop::UpdateState;

#[cfg(mobile)]
#[derive(Default)]
pub struct UpdateState;

#[cfg(desktop)]
pub async fn check(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    desktop::check(app, state).await
}

#[cfg(desktop)]
pub async fn install(app: tauri::AppHandle, state: tauri::State<'_, UpdateState>, expected_version: String) -> Result<(), String> {
    desktop::install(app, state, expected_version).await
}

#[cfg(mobile)]
pub async fn check(
    _app: tauri::AppHandle,
    _state: tauri::State<'_, UpdateState>,
) -> Result<Option<UpdateInfo>, String> {
    Ok(None)
}

#[cfg(mobile)]
pub async fn install(_app: tauri::AppHandle, _state: tauri::State<'_, UpdateState>, _expected_version: String) -> Result<(), String> {
    Err("update_cannot_install".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_linux_appimage_updates_in_place() {
        assert!(can_self_install(true, true));
        assert!(!can_self_install(true, false));
        assert!(can_self_install(false, false));
    }

    /// The updater refuses every download if this key does not decode, and nothing else would
    /// notice until the first user tried to update.
    #[test]
    fn the_configured_public_key_decodes() {
        use base64::Engine;
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        let key = base64::engine::general_purpose::STANDARD
            .decode(updater["pubkey"].as_str().unwrap())
            .expect("the pubkey is base64");
        minisign_verify::PublicKey::decode(std::str::from_utf8(&key).unwrap())
            .expect("the pubkey is a minisign public key");
        let endpoints = updater["endpoints"].as_array().unwrap();
        assert!(endpoints.iter().all(|e| e.as_str().is_some_and(|url| url.starts_with("https://"))));
    }

    #[test]
    fn the_page_receives_camel_case_fields() {
        let info = UpdateInfo {
            version: "0.6.0".into(),
            current_version: "0.5.1".into(),
            notes: Some("Added updates.".into()),
            date: Some("2026-11-01".into()),
            can_install: true,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["currentVersion"], "0.5.1");
        assert_eq!(json["canInstall"], true);
    }

    #[test]
    fn progress_events_name_their_phase() {
        let json = serde_json::to_value(UpdateProgress::Downloading { downloaded: 10, total: Some(40) }).unwrap();
        assert_eq!(json["phase"], "downloading");
        assert_eq!(json["downloaded"], 10);
        assert_eq!(serde_json::to_value(UpdateProgress::Installing).unwrap()["phase"], "installing");
    }
}
