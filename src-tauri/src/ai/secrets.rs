//! API key storage in the operating system's credential store. Keys stay in Rust:
//! no command returns a key to the webview.

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
    /// Name of the store as the user knows it, shown in Settings.
    fn label(&self) -> &'static str;
}

const SERVICE: &str = "Jotter";

pub struct OsSecretStore;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
impl SecretStore for OsSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Cannot read the API key from {}: {e}", self.label())),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        entry(account)?
            .set_password(secret)
            .map_err(|e| format!("Cannot save the API key to {}: {e}", self.label()))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Cannot remove the API key from {}: {e}", self.label())),
        }
    }

    fn label(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "Windows Credential Manager"
        } else if cfg!(target_os = "macos") {
            "the macOS Keychain"
        } else {
            "the system keyring"
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| format!("Cannot open the credential store: {e}"))
}

/// Platforms without a supported credential store refuse to save keys rather than
/// falling back to a plain file.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl SecretStore for OsSecretStore {
    fn get(&self, _account: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn set(&self, _account: &str, _secret: &str) -> Result<(), String> {
        Err(format!("{SERVICE} cannot store API keys on this platform yet."))
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        Ok(())
    }

    fn label(&self) -> &'static str {
        "no credential store"
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MemorySecretStore {
    pub secrets: std::sync::Mutex<std::collections::HashMap<String, String>>,
    pub fail: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl SecretStore for MemorySecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("store unavailable".into());
        }
        Ok(self.secrets.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("store unavailable".into());
        }
        self.secrets.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.secrets.lock().unwrap().remove(account);
        Ok(())
    }

    fn label(&self) -> &'static str {
        "test store"
    }
}
