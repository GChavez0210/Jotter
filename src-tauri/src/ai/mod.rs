//! Bring-your-own AI model: summarize, rewrite, and translate through the user's own
//! provider. Off by default. Keys live in the OS credential store and reach only the
//! endpoint origin they were saved for.

pub mod config;
pub mod guard;
pub mod prompt;
pub mod provider;
pub mod secrets;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use config::{AiConfig, AiProvider, StoredAiSettings};
use prompt::AiOperation;
use provider::{AiUsage, ChatCall};
use secrets::SecretStore;

/// About 25,000 words. Larger notes need a selection.
pub const MAX_INPUT_CHARS: usize = 100_000;

pub struct AiState {
    settings_path: PathBuf,
    settings: Mutex<StoredAiSettings>,
    secrets: Arc<dyn SecretStore>,
    client: reqwest::Client,
    running: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiKeyState {
    None,
    Saved,
    /// A key is saved, but for a different origin than the current base URL.
    EndpointChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyStatus {
    pub provider: AiProvider,
    pub state: AiKeyState,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub config: AiConfig,
    pub keys: Vec<AiKeyStatus>,
    pub credential_store: String,
    pub default_style_instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequest {
    pub operation_id: String,
    pub operation: AiOperation,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiResult {
    pub operation_id: String,
    pub text: String,
    pub provider: AiProvider,
    pub model: String,
    pub usage: AiUsage,
}

pub const CANCELLED: &str = "Cancelled.";

/// Everything one request needs, copied out of the settings lock.
struct Prepared {
    provider: AiProvider,
    base_url: url::Url,
    model: String,
    style: String,
    api_key: Option<String>,
}

/// Bind the secret to its origin inside the credential store as well as in settings.
/// A failed settings write must never authorize a key for a different endpoint.
#[derive(Serialize, Deserialize)]
struct StoredCredential {
    origin: String,
    key: String,
}

impl AiState {
    pub fn new(settings_path: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        let settings = config::load_settings(&settings_path);
        Self {
            settings_path,
            settings: Mutex::new(settings),
            secrets,
            client: provider::build_client(),
            running: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, StoredAiSettings>, String> {
        self.settings.lock().map_err(|_| "AI settings are unavailable.".to_string())
    }

    pub fn status(&self) -> Result<AiStatus, String> {
        let settings = self.lock()?;
        Ok(status_of(&settings, self.secrets.label()))
    }

    pub fn save_config(&self, config: AiConfig) -> Result<AiStatus, String> {
        let config = config.normalized()?;
        let mut settings = self.lock()?;
        let mut next = settings.clone();
        next.config = config;
        config::save_settings(&self.settings_path, &next)?;
        *settings = next;
        Ok(status_of(&settings, self.secrets.label()))
    }

    /// Saves `key` for the provider's current base URL. Sending to another origin later
    /// requires entering the key again.
    pub fn save_key(&self, provider: AiProvider, key: &str) -> Result<AiStatus, String> {
        if !provider.uses_key() {
            return Err(format!("{} does not use an API key.", provider.label()));
        }
        let key = key.trim();
        if key.is_empty() {
            return Err("Enter an API key.".into());
        }
        if key.len() > 4096 || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("That API key contains spaces or invalid characters. Copy it again from your provider.".into());
        }
        let mut settings = self.lock()?;
        let base = &settings.config.endpoint(provider).base_url;
        if base.is_empty() {
            return Err(format!("Enter the {} base URL before saving a key.", provider.label()));
        }
        let origin = config::origin_of(&config::validate_base_url(provider, base)?);
        let account = provider.secret_account();
        let previous = self.secrets.get(account)?;
        let credential = serde_json::to_string(&StoredCredential { origin: origin.clone(), key: key.into() })
            .map_err(|_| "Cannot serialize the API credential.".to_string())?;
        self.secrets.set(account, &credential)?;
        let mut next = settings.clone();
        next.key_origins.insert(provider, origin);
        if let Err(error) = config::save_settings(&self.settings_path, &next) {
            let rollback = match previous {
                Some(secret) => self.secrets.set(account, &secret),
                None => self.secrets.delete(account),
            };
            return Err(match rollback {
                Ok(()) => error,
                Err(_) => format!("{error} The previous credential could not be restored. Save the API key again before using AI."),
            });
        }
        *settings = next;
        Ok(status_of(&settings, self.secrets.label()))
    }

    pub fn delete_key(&self, provider: AiProvider) -> Result<AiStatus, String> {
        let mut settings = self.lock()?;
        self.secrets.delete(provider.secret_account())?;
        let mut next = settings.clone();
        next.key_origins.remove(&provider);
        config::save_settings(&self.settings_path, &next)?;
        *settings = next;
        Ok(status_of(&settings, self.secrets.label()))
    }

    fn prepare(&self) -> Result<Prepared, String> {
        // Keep endpoint authorization and the secret read in the same critical section.
        let settings = self.lock()?;
        let config = &settings.config;
        if !config.enabled {
            return Err("AI is off. Turn it on in Settings > AI.".into());
        }
        let provider = config.provider;
        let endpoint = config.endpoint(provider);
        let label = provider.label();
        if endpoint.base_url.is_empty() {
            return Err(format!("Enter the {label} base URL in Settings > AI."));
        }
        if endpoint.model.is_empty() {
            return Err(format!("Enter a {label} model name in Settings > AI."));
        }
        let base_url = config::validate_base_url(provider, &endpoint.base_url)?;
        let api_key = if provider.uses_key() {
            match settings.key_origins.get(&provider) {
                Some(origin) if *origin == config::origin_of(&base_url) => {
                    match self.secrets.get(provider.secret_account())? {
                        Some(secret) => {
                            let credential = serde_json::from_str::<StoredCredential>(&secret)
                                .map_err(|_| format!("Save the {label} API key again in Settings > AI to bind it to this address."))?;
                            if credential.origin != *origin {
                                return Err(format!("The {label} API key belongs to a different address. Save it again in Settings > AI."));
                            }
                            Some(credential.key)
                        }
                        None => {
                            return Err(format!(
                                "The {label} API key is missing from {}. Save it again in Settings > AI.",
                                self.secrets.label()
                            ))
                        }
                    }
                }
                Some(origin) => {
                    return Err(format!(
                        "The {label} base URL changed since you saved the key for {origin}. Enter the key again in Settings > AI to send it to the new address."
                    ))
                }
                // A server on this computer, such as LM Studio, may not need a key.
                None if config::is_loopback_host(&base_url) => None,
                None => return Err(format!("Save an API key for {label} in Settings > AI.")),
            }
        } else {
            None
        };
        Ok(Prepared {
            provider,
            base_url,
            model: endpoint.model.clone(),
            style: config.style_instructions.clone(),
            api_key,
        })
    }

    pub async fn run(&self, request: AiRequest) -> Result<AiResult, String> {
        let chars = request.text.chars().count();
        if request.text.trim().is_empty() {
            return Err("There is no text to send. Select text or write in the note first.".into());
        }
        if chars > MAX_INPUT_CHARS {
            return Err(format!(
                "The text is too long for one request ({chars} characters; the limit is {MAX_INPUT_CHARS}). Select a shorter passage."
            ));
        }
        let id = request.operation_id.trim().to_string();
        if id.is_empty() || id.len() > 100 {
            return Err("The request has an invalid operation ID.".into());
        }

        let prepared = self.prepare()?;

        let masked = if request.operation.transforms_text() {
            Some(guard::mask_markdown(&request.text)?)
        } else {
            None
        };
        let has_placeholders = masked.as_ref().is_some_and(|m| !m.tokens.is_empty());
        let system = prompt::system_prompt(&request.operation, has_placeholders, &prepared.style)?;
        let user = prompt::user_message(masked.as_ref().map_or(request.text.as_str(), |m| m.text.as_str()));

        let (cancel_tx, cancel_rx) = oneshot::channel();
        {
            let mut running = self.running.lock().map_err(|_| "AI state is unavailable.".to_string())?;
            if running.contains_key(&id) {
                return Err("That request is already running.".into());
            }
            running.insert(id.clone(), cancel_tx);
        }

        let call = ChatCall {
            provider: prepared.provider,
            base_url: &prepared.base_url,
            model: &prepared.model,
            api_key: prepared.api_key.as_deref(),
            system: &system,
            user: &user,
        };
        // Dropping the request future on cancel closes the connection.
        let outcome = tokio::select! {
            reply = provider::send_chat(&self.client, &call) => reply,
            _ = cancel_rx => Err(CANCELLED.to_string()),
        };
        if let Ok(mut running) = self.running.lock() {
            running.remove(&id);
        }
        let (raw, usage) = outcome?;

        let reply = guard::strip_wrapping_fence(&raw, &request.text);
        let text = match &masked {
            Some(masked) => guard::keep_surrounding_whitespace(
                &request.text,
                &guard::unmask_markdown(&reply, &masked.tokens)?,
            ),
            None => reply,
        };
        Ok(AiResult {
            operation_id: id,
            text,
            provider: prepared.provider,
            model: prepared.model,
            usage,
        })
    }

    /// Returns whether a request with that ID was running.
    pub fn cancel(&self, operation_id: &str) -> bool {
        let sender = self
            .running
            .lock()
            .ok()
            .and_then(|mut running| running.remove(operation_id));
        match sender {
            Some(tx) => {
                let _ = tx.send(());
                true
            }
            None => false,
        }
    }
}

fn status_of(settings: &StoredAiSettings, credential_store: &str) -> AiStatus {
    let keys = AiProvider::ALL
        .iter()
        .filter(|p| p.uses_key())
        .map(|&provider| {
            let origin = settings.key_origins.get(&provider).cloned();
            let current = config::validate_base_url(provider, &settings.config.endpoint(provider).base_url)
                .ok()
                .map(|u| config::origin_of(&u));
            let state = match (&origin, &current) {
                (None, _) => AiKeyState::None,
                (Some(saved), Some(now)) if saved == now => AiKeyState::Saved,
                _ => AiKeyState::EndpointChanged,
            };
            AiKeyStatus { provider, state, origin }
        })
        .collect();
    AiStatus {
        config: settings.config.clone(),
        keys,
        credential_store: credential_store.to_string(),
        default_style_instructions: config::default_style_instructions(),
    }
}

#[cfg(test)]
mod tests;
