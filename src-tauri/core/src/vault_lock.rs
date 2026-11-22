//! Coordinates vault mutations between Desktop and the CLI/MCP server.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

/// An OS lock for one vault, shared by Jotter processes using the same app data directory.
/// The OS releases it when the file handle closes, including on process exit.
pub struct VaultWriteLock {
    _file: File,
}

impl VaultWriteLock {
    pub fn acquire(app_data_dir: &Path, vault_root: &Path) -> Result<Self, String> {
        let root = fs::canonicalize(vault_root).map_err(|err| format!("Cannot resolve vault: {err}"))?;
        let folder = app_data_dir.join("vault-locks");
        fs::create_dir_all(&folder).map_err(|err| format!("Cannot create the vault lock folder: {err}"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join(format!("{}.lock", crate::platform::vault_key(&root))))
            .map_err(|err| format!("Cannot open the vault write lock: {err}"))?;

        // Keep the file in place after release. Unlinking it would let another process
        // create and lock a different file while a waiter still holds the original handle.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(TryLockError::WouldBlock) => {
                    return Err("Another Jotter process is writing to this vault. Try again shortly.".into());
                }
                Err(TryLockError::Error(err)) => return Err(format!("Cannot lock the vault: {err}")),
            }
        }
    }
}
