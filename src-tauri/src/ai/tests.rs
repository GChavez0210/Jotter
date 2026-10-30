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
    fake_server_with_hook(reply, delay, || {})
}

fn fake_server_with_hook(reply: serde_json::Value, delay: Duration, hook: impl FnOnce() + Send + 'static) -> (String, JoinHandle<String>) {
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
        hook();
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

#[test]
fn embeddings_use_the_embedding_model_and_the_saved_key() {
    let (base, server) = fake_server(json!({"data":[{"index":0,"embedding":[0.25,0.75]}]}), Duration::ZERO);
    let (_dir, state, _) = state_with(AiProvider::Anthropic, &base);
    state.save_key(AiProvider::OpenaiCompatible, "sk-test").unwrap();
    let mut config = state.status().unwrap().config;
    config.embedding_provider = AiProvider::OpenaiCompatible;
    config.embedding_model = "embed-test".into();
    config.ask_vault_enabled = true;
    state.save_config(config).unwrap();

    let result = block_on(state.embed(&["hello".to_string()])).unwrap();
    assert_eq!(result.vectors, vec![vec![0.25, 0.75]]);
    assert_eq!(result.model, "embed-test");
    let raw = server.join().unwrap();
    assert!(raw.starts_with("POST /embeddings"));
    assert!(raw.to_ascii_lowercase().contains("authorization: bearer sk-test"));
    assert!(raw.contains("\"model\":\"embed-test\""));
}

#[test]
fn embeddings_need_a_model_and_refuse_anthropic() {
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, "http://127.0.0.1:9");
    let err = block_on(state.embed(&["x".to_string()])).unwrap_err();
    assert!(err.contains("Ask your vault is off"));
    let mut config = state.status().unwrap().config;
    config.ask_vault_enabled = true;
    state.save_config(config).unwrap();
    let err = block_on(state.embed(&["x".to_string()])).unwrap_err();
    assert!(err.contains("embedding model"));
    let mut config = state.status().unwrap().config;
    config.embedding_provider = AiProvider::Anthropic;
    assert!(state.save_config(config).is_err());
}

#[test]
fn indexing_embeds_new_notes_once_and_skips_unchanged_ones() {
    let vault = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("Rent.md"), "# Rent\nRent is 900 a month.\n").unwrap();
    let data = tempfile::tempdir().unwrap();

    let (base, server) = fake_server(json!({"data":[{"index":0,"embedding":[1.0,0.0]}]}), Duration::ZERO);
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, &base);
    let mut config = state.status().unwrap().config;
    config.embedding_model = "embed-test".into();
    config.ask_vault_enabled = true;
    state.save_config(config).unwrap();

    let status = ask::index_status(&state, data.path(), vault.path()).unwrap();
    assert_eq!((status.notes_to_embed, status.notes_total), (1, 1));
    let report = block_on(ask::index_vault(&state, data.path(), vault.path())).unwrap();
    assert_eq!((report.embedded_notes, report.passages), (1, 1));
    server.join().unwrap();

    // The fake server is gone, so a second run that tried to embed would fail.
    let again = block_on(ask::index_vault(&state, data.path(), vault.path())).unwrap();
    assert_eq!(again.embedded_notes, 0);
}

fn enable_ask(state: &AiState) -> EmbeddingProfile {
    let mut config = state.status().unwrap().config;
    config.embedding_model = "embed-test".into();
    config.ask_vault_enabled = true;
    state.save_config(config).unwrap();
    state.embedding_profile().unwrap()
}

fn seed_ask_note(state: &AiState, data: &std::path::Path, vault: &std::path::Path) {
    let file = crate::index::scan_dir_files_from_disk(vault, vault).unwrap().remove(0);
    let chunks = crate::semantic::chunk_note(&file.path, &file.content).into_iter()
        .map(|chunk| crate::semantic::StoredChunk { chunk, vector: vec![1.0, 0.0] }).collect::<Vec<_>>();
    crate::vector_store::VectorStore::open(&ask::store_path(data, vault)).unwrap()
        .replace_note(&file.path, file.fs_modified_ms, &state.embedding_profile().unwrap().identity(), &chunks).unwrap();
}

#[test]
fn embedding_identity_includes_provider_and_full_endpoint() {
    let vault = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("A.md"), "Body").unwrap();
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, "http://127.0.0.1:9/v1");
    let original = enable_ask(&state);
    seed_ask_note(&state, data.path(), vault.path());
    assert_eq!(ask::index_status(&state, data.path(), vault.path()).unwrap().notes_to_embed, 0);
    let mut config = state.status().unwrap().config;
    config.embedding_provider = AiProvider::Ollama;
    state.save_config(config).unwrap();
    assert_ne!(state.embedding_profile().unwrap().identity(), original.identity());
    assert_eq!(ask::index_status(&state, data.path(), vault.path()).unwrap().notes_to_embed, 1);
    let mut config = state.status().unwrap().config;
    config.embedding_provider = AiProvider::OpenaiCompatible;
    config.openai_compatible.base_url = "http://127.0.0.1:9/other".into();
    state.save_config(config).unwrap();
    assert_eq!(ask::index_status(&state, data.path(), vault.path()).unwrap().notes_to_embed, 1);
}

#[test]
fn deleted_and_modified_notes_are_never_used_as_answer_sources() {
    let vault = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let note = vault.path().join("A.md");
    std::fs::write(&note, "Sensitive text").unwrap();
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, "http://127.0.0.1:9");
    let profile = enable_ask(&state);
    seed_ask_note(&state, data.path(), vault.path());
    std::fs::remove_file(&note).unwrap();
    assert_eq!(ask::index_status(&state, data.path(), vault.path()).unwrap().notes_to_remove, 1);
    assert!(block_on(ask::ask(&state, data.path(), vault.path(), "Question")).unwrap_err().contains("Index the vault first"));
    let mut store = crate::vector_store::VectorStore::open(&ask::store_path(data.path(), vault.path())).unwrap();
    assert!(store.load(&profile.identity()).unwrap().is_empty());

    std::fs::write(&note, "Current text").unwrap();
    let chunk = crate::semantic::StoredChunk { chunk: crate::semantic::Chunk {
        path: "A.md".into(), heading: String::new(), line: 1, text: "Old text".into(),
    }, vector: vec![1.0, 0.0] };
    store.replace_note("A.md", 1, &profile.identity(), &[chunk]).unwrap();
    assert!(block_on(ask::ask(&state, data.path(), vault.path(), "Question")).unwrap_err().contains("Index the vault first"));
}

#[test]
fn turning_ask_off_during_query_embedding_prevents_chat() {
    let vault = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("A.md"), "Sensitive text").unwrap();
    let state_slot = Arc::new(Mutex::new(None::<Arc<AiState>>));
    let slot = state_slot.clone();
    let (base, server) = fake_server_with_hook(json!({"data":[{"index":0,"embedding":[1.0,0.0]}]}), Duration::ZERO, move || {
        let state = slot.lock().unwrap().clone().unwrap();
        let mut config = state.status().unwrap().config;
        config.ask_vault_enabled = false;
        state.save_config(config).unwrap();
    });
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, &base);
    let state = Arc::new(state);
    let profile = enable_ask(&state);
    seed_ask_note(&state, data.path(), vault.path());
    *state_slot.lock().unwrap() = Some(state.clone());
    let error = block_on(ask::ask(&state, data.path(), vault.path(), "Question")).unwrap_err();
    assert_eq!(error, ASK_OFF);
    assert!(server.join().unwrap().starts_with("POST /embeddings"));
    assert_eq!(block_on(state.chat_for_vault(&profile, "System", "Sensitive text")).unwrap_err(), ASK_OFF);
    assert!(block_on(state.embed(&[])).is_err());
}

#[test]
fn changing_models_during_indexing_never_stores_mislabeled_vectors() {
    let vault = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(vault.path().join("A.md"), "First note").unwrap();
    std::fs::write(vault.path().join("B.md"), "Second note").unwrap();
    let state_slot = Arc::new(Mutex::new(None::<Arc<AiState>>));
    let slot = state_slot.clone();
    let (base, server) = fake_server_with_hook(json!({"data":[{"index":0,"embedding":[1.0,0.0]}]}), Duration::ZERO, move || {
        let state = slot.lock().unwrap().clone().unwrap();
        let mut config = state.status().unwrap().config;
        config.embedding_model = "other-model".into();
        state.save_config(config).unwrap();
    });
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, &base);
    let state = Arc::new(state);
    let profile = enable_ask(&state);
    *state_slot.lock().unwrap() = Some(state.clone());
    assert!(block_on(ask::index_vault(&state, data.path(), vault.path())).unwrap_err().contains("settings changed"));
    server.join().unwrap();
    let store = crate::vector_store::VectorStore::open(&ask::store_path(data.path(), vault.path())).unwrap();
    assert!(store.load(&profile.identity()).unwrap().is_empty());
    assert!(store.load(&state.embedding_profile().unwrap().identity()).unwrap().is_empty());
}

#[test]
fn deleting_a_note_during_question_embedding_prevents_sending_it_to_chat() {
    let vault = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let note = vault.path().join("A.md");
    std::fs::write(&note, "Sensitive text").unwrap();
    let (base, server) = fake_server_with_hook(json!({"data":[{"index":0,"embedding":[1.0,0.0]}]}), Duration::ZERO, move || {
        std::fs::remove_file(note).unwrap();
    });
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, &base);
    enable_ask(&state);
    seed_ask_note(&state, data.path(), vault.path());
    assert!(block_on(ask::ask(&state, data.path(), vault.path(), "Question")).unwrap_err().contains("deleted"));
    server.join().unwrap();
}

#[test]
fn turning_ask_off_stops_later_embedding_batches() {
    let state_slot = Arc::new(Mutex::new(None::<Arc<AiState>>));
    let slot = state_slot.clone();
    let rows = (0..EMBED_BATCH).map(|i| json!({"index": i, "embedding": [1.0, 0.0]})).collect::<Vec<_>>();
    let (base, server) = fake_server_with_hook(json!({"data": rows}), Duration::ZERO, move || {
        let state = slot.lock().unwrap().clone().unwrap();
        let mut config = state.status().unwrap().config;
        config.ask_vault_enabled = false;
        state.save_config(config).unwrap();
    });
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, &base);
    let state = Arc::new(state);
    enable_ask(&state);
    *state_slot.lock().unwrap() = Some(state.clone());
    assert_eq!(block_on(state.embed(&vec!["Text".to_string(); EMBED_BATCH + 1])).unwrap_err(), ASK_OFF);
    server.join().unwrap();
}

#[test]
fn embedding_requests_keep_origin_bound_keys_and_respect_the_general_ai_switch() {
    let (_dir, state, _) = state_with(AiProvider::OpenaiCompatible, "http://127.0.0.1:9");
    enable_ask(&state);
    state.save_key(AiProvider::OpenaiCompatible, "sk-test").unwrap();
    let mut config = state.status().unwrap().config;
    config.openai_compatible.base_url = "http://127.0.0.1:10".into();
    state.save_config(config).unwrap();
    assert!(block_on(state.embed(&["Secret".into()])).unwrap_err().contains("base URL changed"));
    let mut config = state.status().unwrap().config;
    config.enabled = false;
    state.save_config(config).unwrap();
    assert!(state.embedding_profile().unwrap_err().contains("AI is off"));
    assert!(block_on(state.embed(&["Secret".into()])).unwrap_err().contains("AI is off"));
}
