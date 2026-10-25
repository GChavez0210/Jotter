use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::json;

use super::config::AiProvider;
use super::prompt::{AiOperation, RewriteStyle};
use super::secrets::MemorySecretStore;
use super::*;

/// Serves one HTTP request on 127.0.0.1 and returns the raw request it received.
fn fake_server(reply: serde_json::Value, delay: Duration) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&raw);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                    .unwrap_or(0);
                if raw.len() >= end + 4 + length {
                    break;
                }
            }
        }
        std::thread::sleep(delay);
        let body = reply.to_string();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        String::from_utf8_lossy(&raw).into_owned()
    });
    (base, handle)
}

fn state_with(provider: AiProvider, base: &str) -> (tempfile::TempDir, AiState, Arc<MemorySecretStore>) {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecretStore::default());
    let state = AiState::new(config::settings_path(dir.path()), secrets.clone());
    let mut config = AiConfig { enabled: true, provider, ..AiConfig::default() };
    config.openai_compatible.base_url = base.into();
    config.anthropic.base_url = base.into();
    config.ollama.base_url = base.into();
    config.openai_compatible.model = "gpt-test".into();
    config.anthropic.model = "claude-test".into();
    config.ollama.model = "llama-test".into();
    state.save_config(config).unwrap();
    (dir, state, secrets)
}

fn request(operation: AiOperation, text: &str) -> AiRequest {
    AiRequest { operation_id: "op-1".into(), operation, text: text.into() }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

#[test]
fn translate_sends_the_key_and_restores_protected_markdown() {
    let (base, server) = fake_server(
        json!({"choices":[{"message":{"content":"Ejecuta ⟦J0⟧ y abre ⟦J1⟧."},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":4}}),
        Duration::ZERO,
    );
    let (_dir, state, secrets) = state_with(AiProvider::OpenaiCompatible, &base);
    state.save_key(AiProvider::OpenaiCompatible, "sk-test").unwrap();
    assert!(!state.status().unwrap().config.style_instructions.is_empty());
    let credential: StoredCredential = serde_json::from_str(&secrets.secrets.lock().unwrap()["ai-openai-compatible"]).unwrap();
    assert_eq!(credential.key, "sk-test");
    assert_eq!(credential.origin, base);

    let op = AiOperation::Translate { target_language: "Spanish".into() };
    let result = block_on(state.run(request(op, "Run `make` and open [[Plan]].\n"))).unwrap();
    assert_eq!(result.text, "Ejecuta `make` y abre [[Plan]].\n");
    assert_eq!(result.usage.output_tokens, Some(4));

    let raw = server.join().unwrap();
    assert!(raw.starts_with("POST /chat/completions"));
    assert!(raw.to_ascii_lowercase().contains("authorization: bearer sk-test"));
    assert!(raw.contains("Run ⟦J0⟧ and open ⟦J1⟧."));
    assert!(!raw.contains("[[Plan]]"));
}

#[test]
fn key_is_withheld_after_the_endpoint_changes() {
    let (_dir, state, _) = state_with(AiProvider::Anthropic, "https://api.anthropic.com");
    state.save_key(AiProvider::Anthropic, "sk-ant").unwrap();
    let mut config = state.status().unwrap().config;
    config.anthropic.base_url = "https://proxy.example.com".into();
    let status = state.save_config(config).unwrap();
    let key = status.keys.iter().find(|k| k.provider == AiProvider::Anthropic).unwrap();
    assert_eq!(key.state, AiKeyState::EndpointChanged);

    let err = block_on(state.run(request(AiOperation::Summarize, "Hello"))).unwrap_err();
    assert!(err.contains("changed since you saved the key"), "{err}");
}

#[test]
fn anthropic_uses_its_own_headers_and_summary_ignores_placeholders() {
    let (base, server) = fake_server(
        json!({"content":[{"type":"text","text":"- One point"}],"stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":2}}),
        Duration::ZERO,
    );
    let (_dir, state, _) = state_with(AiProvider::Anthropic, &base);
    state.save_key(AiProvider::Anthropic, "sk-ant").unwrap();
    let result = block_on(state.run(request(AiOperation::Summarize, "Use `x`."))).unwrap();
    assert_eq!(result.text, "- One point");
    let raw = server.join().unwrap().to_ascii_lowercase();
    assert!(raw.starts_with("post /v1/messages"));
    assert!(raw.contains("x-api-key: sk-ant") && raw.contains("anthropic-version: 2023-06-01"));
    assert!(!raw.contains("authorization:"));
    assert!(raw.contains("use `x`."));
}

#[test]
fn local_servers_run_without_a_key_but_cloud_ones_do_not() {
    let (base, server) = fake_server(json!({"message":{"content":"Clean text"},"done_reason":"stop"}), Duration::ZERO);
    let (_dir, state, _) = state_with(AiProvider::Ollama, &base);
    let polish = AiOperation::Rewrite { style: RewriteStyle::Polish };
    assert_eq!(block_on(state.run(request(polish, "messy text"))).unwrap().text, "Clean text");
    let raw = server.join().unwrap();
    assert!(raw.starts_with("POST /api/chat") && !raw.to_ascii_lowercase().contains("authorization:"));
    assert!(raw.contains("<style>"), "Polish sends the writing style");

    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, "https://api.openai.com/v1");
    let err = block_on(state.run(request(AiOperation::Summarize, "Hi"))).unwrap_err();
    assert!(err.contains("Save an API key for OpenAI-compatible"), "{err}");
}

#[test]
fn a_reply_that_drops_protected_markdown_is_rejected() {
    let (base, _server) = fake_server(json!({"message":{"content":"Hola"},"done_reason":"stop"}), Duration::ZERO);
    let (_dir, state, _) = state_with(AiProvider::Ollama, &base);
    let op = AiOperation::Translate { target_language: "Spanish".into() };
    let err = block_on(state.run(request(op, "Hi `code`"))).unwrap_err();
    assert!(err.contains("left the note unchanged"), "{err}");
}

#[test]
fn cancel_stops_a_running_request() {
    let (base, _server) = fake_server(json!({"message":{"content":"late"}}), Duration::from_secs(4));
    let (_dir, state, _) = state_with(AiProvider::Ollama, &base);
    let state = Arc::new(state);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let started = Instant::now();
    let canceller = Arc::clone(&state);
    let result = runtime.block_on(async move {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(canceller.cancel("op-1"));
        });
        state.run(request(AiOperation::Summarize, "Hello")).await
    });
    assert_eq!(result.unwrap_err(), CANCELLED);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn requests_fail_safely_when_off_empty_too_long_or_the_store_breaks() {
    let (_dir, state, secrets) = state_with(AiProvider::Anthropic, "https://api.anthropic.com");
    state.save_key(AiProvider::Anthropic, "sk-ant").unwrap();
    assert!(block_on(state.run(request(AiOperation::Summarize, "  "))).is_err());
    let long = "a".repeat(MAX_INPUT_CHARS + 1);
    assert!(block_on(state.run(request(AiOperation::Summarize, &long))).unwrap_err().contains("too long"));

    secrets.fail.store(true, Ordering::SeqCst);
    assert!(block_on(state.run(request(AiOperation::Summarize, "Hi"))).unwrap_err().contains("store unavailable"));

    let mut config = state.status().unwrap().config;
    config.enabled = false;
    state.save_config(config).unwrap();
    assert!(block_on(state.run(request(AiOperation::Summarize, "Hi"))).unwrap_err().contains("AI is off"));
}

#[test]
fn keys_are_validated_and_removed() {
    let (_dir, state, secrets) = state_with(AiProvider::Anthropic, "https://api.anthropic.com");
    assert!(state.save_key(AiProvider::Anthropic, "sk ant").is_err());
    assert!(state.save_key(AiProvider::Ollama, "x").is_err());
    state.save_key(AiProvider::Anthropic, " sk-ant ").unwrap();
    let credential: StoredCredential = serde_json::from_str(&secrets.secrets.lock().unwrap()["ai-anthropic"]).unwrap();
    assert_eq!(credential.key, "sk-ant");
    let status = state.delete_key(AiProvider::Anthropic).unwrap();
    assert!(secrets.secrets.lock().unwrap().is_empty());
    assert!(status.keys.iter().all(|k| k.state == AiKeyState::None));
}

#[test]
fn failed_key_metadata_save_restores_the_previous_credential() {
    for had_key in [false, true] {
        let (_dir, state, secrets) = state_with(AiProvider::Anthropic, "https://old.example");
        if had_key {
            state.save_key(AiProvider::Anthropic, "old-key").unwrap();
        }
        let previous = secrets.get("ai-anthropic").unwrap();
        let mut config = state.status().unwrap().config;
        config.anthropic.base_url = "https://new.example".into();
        state.save_config(config).unwrap();
        let saved_settings = std::fs::read(&state.settings_path).unwrap();
        // A directory at the destination forces atomic rename to fail on every OS.
        std::fs::remove_file(&state.settings_path).unwrap();
        std::fs::create_dir(&state.settings_path).unwrap();
        assert!(state.save_key(AiProvider::Anthropic, "new-key").is_err());
        assert_eq!(secrets.get("ai-anthropic").unwrap(), previous);
        std::fs::remove_dir(&state.settings_path).unwrap();
        std::fs::write(&state.settings_path, saved_settings).unwrap();
        let restarted = AiState::new(state.settings_path.clone(), secrets);
        let mut config = restarted.status().unwrap().config;
        config.anthropic.base_url = "https://old.example".into();
        restarted.save_config(config).unwrap();
        if had_key {
            assert_eq!(restarted.prepare().unwrap().api_key.as_deref(), Some("old-key"));
        } else {
            assert!(restarted.prepare().is_err());
        }
    }
}

#[test]
fn credential_origin_is_checked_even_if_settings_still_authorize_the_old_address() {
    let (_dir, state, secrets) = state_with(AiProvider::Anthropic, "https://old.example");
    state.save_key(AiProvider::Anthropic, "old-key").unwrap();
    // Simulate a failed settings write followed by a failed credential rollback.
    let credential = StoredCredential { origin: "https://new.example".into(), key: "new-key".into() };
    secrets.set("ai-anthropic", &serde_json::to_string(&credential).unwrap()).unwrap();
    let error = state.prepare().err().unwrap();
    assert!(error.contains("different address"), "{error}");
    let restarted = AiState::new(state.settings_path.clone(), secrets);
    assert!(restarted.prepare().is_err());
}
