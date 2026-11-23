//! Native dialog approvals are stored separately from importable WebView settings.
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DestinationKind {
    Calendar,
    Backup,
}

impl DestinationKind {
    fn name(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::Backup => "backup",
        }
    }

    fn record(self, app_data: &Path, vault: &Path) -> Result<PathBuf, String> {
        let vault = vault.canonicalize().map_err(|err| format!("Cannot resolve vault: {err}"))?;
        Ok(app_data.join("export-destinations").join(format!(
            "{}-{}.json", crate::platform::vault_key(&vault), self.name()
        )))
    }

    fn resolve(self, path: &Path) -> Result<PathBuf, String> {
        if !path.is_absolute() {
            return Err("Choose an absolute destination using the native picker.".into());
        }
        match self {
            Self::Backup => {
                let resolved = path.canonicalize().map_err(|err| format!("Cannot resolve backup folder: {err}"))?;
                if !resolved.is_dir() {
                    return Err("Choose a folder for the backups.".into());
                }
                Ok(resolved)
            }
            Self::Calendar => {
                if !path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("ics")) {
                    return Err("Choose an .ics file for the task calendar.".into());
                }
                match fs::symlink_metadata(path) {
                    Ok(meta) if meta.is_file() && !crate::vault::is_link_metadata(&meta) => {}
                    Err(err) if err.kind() == ErrorKind::NotFound => {}
                    _ => return Err("The calendar destination must be a regular file.".into()),
                }
                let parent = path.parent().ok_or("The calendar file needs a parent folder.")?;
                let parent = parent.canonicalize().map_err(|err| format!("Cannot resolve calendar folder: {err}"))?;
                Ok(parent.join(path.file_name().ok_or("The calendar file needs a name.")?))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Approval {
    selected: PathBuf,
    resolved: PathBuf,
}

pub(crate) fn approve(app_data: &Path, vault: &Path, kind: DestinationKind, selected: &Path) -> Result<(), String> {
    let approval = Approval { selected: selected.into(), resolved: kind.resolve(selected)? };
    let json = serde_json::to_string(&approval).map_err(|err| format!("Cannot record export destination: {err}"))?;
    crate::fs_atomic::atomic_write_str(&kind.record(app_data, vault)?, &json)
}

pub(crate) fn require_approval(
    app_data: &Path,
    vault: &Path,
    kind: DestinationKind,
    requested: &Path,
) -> Result<PathBuf, String> {
    let error = || format!("Choose the {} destination again using Jotter’s picker.", kind.name());
    let raw = fs::read_to_string(kind.record(app_data, vault)?).map_err(|_| error())?;
    let approval: Approval = serde_json::from_str(&raw).map_err(|_| error())?;
    if requested != approval.selected || kind.resolve(requested)? != approval.resolved {
        return Err(error());
    }
    Ok(approval.resolved)
}

pub(crate) fn revoke(app_data: &Path, vault: &Path, kind: DestinationKind) -> Result<(), String> {
    match fs::remove_file(kind.record(app_data, vault)?) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("Cannot forget the export destination: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_approval_is_specific_to_the_vault_path_and_operation() {
        let data = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let other_vault = tempfile::tempdir().unwrap();
        let target = data.path().join("calendar.ics");
        let kind = DestinationKind::Calendar;
        assert!(require_approval(data.path(), vault.path(), kind, &target).is_err());
        approve(data.path(), vault.path(), kind, &target).unwrap();
        assert!(require_approval(data.path(), vault.path(), kind, &target).is_ok());
        assert!(require_approval(data.path(), vault.path(), kind, &data.path().join("other.ics")).is_err());
        assert!(require_approval(data.path(), other_vault.path(), kind, &target).is_err());
        assert!(require_approval(data.path(), vault.path(), DestinationKind::Backup, &target).is_err());
        revoke(data.path(), vault.path(), kind).unwrap();
        assert!(require_approval(data.path(), vault.path(), kind, &target).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_retargeted_picker_path_requires_a_new_approval() {
        let data = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let destination = data.path().join("selected");
        std::os::unix::fs::symlink(parent.path(), &destination).unwrap();
        let calendar = destination.join("tasks.ics");
        approve(data.path(), vault.path(), DestinationKind::Calendar, &calendar).unwrap();
        approve(data.path(), vault.path(), DestinationKind::Backup, &destination).unwrap();
        fs::remove_file(&destination).unwrap();
        std::os::unix::fs::symlink(other.path(), &destination).unwrap();
        assert!(require_approval(data.path(), vault.path(), DestinationKind::Calendar, &calendar).is_err());
        assert!(require_approval(data.path(), vault.path(), DestinationKind::Backup, &destination).is_err());
    }
}
