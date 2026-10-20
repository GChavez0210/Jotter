//! Provider adapters. Each one builds a chat request and reads the reply and token usage
//! from that provider's JSON shape. Requests go straight from this computer to the provider.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use super::config::AiProvider;

/// Anthropic requires an output limit. 8,192 tokens covers about 25,000 characters of reply.
const ANTHROPIC_MAX_TOKENS: u32 = 8192;
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

pub struct ChatCall<'a> {
    pub provider: AiProvider,
    pub base_url: &'a Url,
    pub model: &'a str,
    pub api_key: Option<&'a str>,
    pub system: &'a str,
    pub user: &'a str,
}

pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        // A redirect could carry the key to another host, so never follow one.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build the HTTP client")
}

pub fn request_url(provider: AiProvider, base_url: &Url) -> Url {
    let suffix = match provider {
        AiProvider::OpenaiCompatible => "chat/completions",
        AiProvider::Anthropic => "v1/messages",
        AiProvider::Ollama => "api/chat",
    };
    let mut url = base_url.clone();
    let path = format!("{}/{suffix}", url.path().trim_end_matches('/'));
    url.set_path(&path);
    url
}

pub fn request_body(call: &ChatCall<'_>) -> Value {
    match call.provider {
        AiProvider::OpenaiCompatible => json!({
            "model": call.model,
            "messages": [
                {"role": "system", "content": call.system},
                {"role": "user", "content": call.user},
            ],
        }),
        AiProvider::Anthropic => json!({
            "model": call.model,
            "max_tokens": ANTHROPIC_MAX_TOKENS,
            "system": call.system,
            "messages": [{"role": "user", "content": call.user}],
        }),
        AiProvider::Ollama => json!({
            "model": call.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": call.system},
                {"role": "user", "content": call.user},
            ],
        }),
    }
}

const CUT_OFF: &str = "The reply was cut off because it reached the model's output limit. Jotter left the note unchanged. Select less text and try again.";

pub fn parse_reply(provider: AiProvider, body: &Value) -> Result<(String, AiUsage), String> {
    let missing = || format!("{} returned a reply Jotter cannot read.", provider.label());
    let (text, stop, usage) = match provider {
        AiProvider::OpenaiCompatible => {
            let choice = body.pointer("/choices/0").ok_or_else(missing)?;
            let text = choice
                .pointer("/message/content")
                .and_then(Value::as_str)
                .ok_or_else(missing)?
                .to_string();
            let usage = AiUsage {
                input_tokens: body.pointer("/usage/prompt_tokens").and_then(Value::as_u64),
                output_tokens: body.pointer("/usage/completion_tokens").and_then(Value::as_u64),
            };
            (text, choice.get("finish_reason").and_then(Value::as_str).map(str::to_string), usage)
        }
        AiProvider::Anthropic => {
            let blocks = body.get("content").and_then(Value::as_array).ok_or_else(missing)?;
            let text: String = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            let usage = AiUsage {
                input_tokens: body.pointer("/usage/input_tokens").and_then(Value::as_u64),
                output_tokens: body.pointer("/usage/output_tokens").and_then(Value::as_u64),
            };
            (text, body.get("stop_reason").and_then(Value::as_str).map(str::to_string), usage)
        }
        AiProvider::Ollama => {
            let text = body
                .pointer("/message/content")
                .and_then(Value::as_str)
                .ok_or_else(missing)?
                .to_string();
            let usage = AiUsage {
                input_tokens: body.get("prompt_eval_count").and_then(Value::as_u64),
                output_tokens: body.get("eval_count").and_then(Value::as_u64),
            };
            (text, body.get("done_reason").and_then(Value::as_str).map(str::to_string), usage)
        }
    };
    if matches!(stop.as_deref(), Some("length" | "max_tokens")) {
        return Err(CUT_OFF.into());
    }
    if text.trim().is_empty() {
        return Err(format!("{} returned an empty reply.", provider.label()));
    }
    Ok((text, usage))
}

/// Turns an HTTP error into a message that says what to check. Never includes request headers.
pub fn describe_http_error(provider: AiProvider, status: u16, body: &str) -> String {
    let label = provider.label();
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .map(|m| {
            let short: String = m.chars().take(300).collect();
            format!(" {label} said: {short}")
        })
        .unwrap_or_default();
    let hint = match status {
        401 | 403 => "The provider rejected the API key. Check the key in Settings > AI.",
        404 => "The provider did not find the endpoint or model. Check the base URL and model name.",
        429 => "The provider's rate limit or quota is reached. Wait, or check your plan with the provider.",
        300..=399 => "The provider redirected the request. Jotter does not follow redirects; enter the final base URL.",
        500..=599 => "The provider had a server error. Try again later.",
        _ => "The provider refused the request.",
    };
    format!("{hint} (HTTP {status}){detail}")
}

pub fn describe_transport_error(provider: AiProvider, base_url: &Url, err: &reqwest::Error) -> String {
    let host = base_url.host_str().unwrap_or("the provider");
    if err.is_timeout() {
        format!("{host} did not respond in time. Try again, or select less text.")
    } else if err.is_connect() {
        if provider == AiProvider::Ollama {
            format!("Cannot reach Ollama at {host}. Check that Ollama is running and the base URL is correct.")
        } else {
            format!("Cannot reach {host}. Check your connection and the base URL.")
        }
    } else {
        format!("The request to {host} failed. Check your connection and the base URL.")
    }
}

pub async fn send_chat(client: &reqwest::Client, call: &ChatCall<'_>) -> Result<(String, AiUsage), String> {
    let url = request_url(call.provider, call.base_url);
    let mut request = client.post(url).json(&request_body(call));
    match (call.provider, call.api_key) {
        (AiProvider::Anthropic, Some(key)) => {
            request = request.header("x-api-key", key).header("anthropic-version", ANTHROPIC_VERSION);
        }
        (AiProvider::Anthropic, None) => {
            request = request.header("anthropic-version", ANTHROPIC_VERSION);
        }
        (_, Some(key)) => request = request.bearer_auth(key),
        (_, None) => {}
    }
    let response = request
        .send()
        .await
        .map_err(|e| describe_transport_error(call.provider, call.base_url, &e))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| describe_transport_error(call.provider, call.base_url, &e))?;
    if !status.is_success() {
        return Err(describe_http_error(call.provider, status.as_u16(), &body));
    }
    let json: Value = serde_json::from_str(&body)
        .map_err(|_| format!("{} returned a reply Jotter cannot read.", call.provider.label()))?;
    parse_reply(call.provider, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call<'a>(provider: AiProvider, url: &'a Url) -> ChatCall<'a> {
        ChatCall { provider, base_url: url, model: "m", api_key: Some("k"), system: "sys", user: "hi" }
    }

    #[test]
    fn urls_append_each_provider_path() {
        let base = Url::parse("https://api.openai.com/v1").unwrap();
        assert_eq!(request_url(AiProvider::OpenaiCompatible, &base).as_str(), "https://api.openai.com/v1/chat/completions");
        let base = Url::parse("https://api.anthropic.com").unwrap();
        assert_eq!(request_url(AiProvider::Anthropic, &base).as_str(), "https://api.anthropic.com/v1/messages");
        let base = Url::parse("http://localhost:11434/").unwrap();
        assert_eq!(request_url(AiProvider::Ollama, &base).as_str(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn bodies_match_each_api() {
        let url = Url::parse("https://x.test").unwrap();
        let openai = request_body(&call(AiProvider::OpenaiCompatible, &url));
        assert_eq!(openai["messages"][0]["role"], "system");
        assert!(openai.get("max_tokens").is_none());
        let anthropic = request_body(&call(AiProvider::Anthropic, &url));
        assert_eq!(anthropic["system"], "sys");
        assert_eq!(anthropic["messages"].as_array().unwrap().len(), 1);
        assert_eq!(anthropic["max_tokens"], ANTHROPIC_MAX_TOKENS);
        assert_eq!(request_body(&call(AiProvider::Ollama, &url))["stream"], false);
    }

    #[test]
    fn replies_and_usage_parse_for_each_provider() {
        let openai = json!({"choices":[{"message":{"content":"Hola"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}});
        assert_eq!(
            parse_reply(AiProvider::OpenaiCompatible, &openai).unwrap(),
            ("Hola".into(), AiUsage { input_tokens: Some(5), output_tokens: Some(2) })
        );
        let anthropic = json!({"content":[{"type":"text","text":"Ho"},{"type":"text","text":"la"}],"stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":3}});
        assert_eq!(parse_reply(AiProvider::Anthropic, &anthropic).unwrap().0, "Hola");
        let ollama = json!({"message":{"content":"Hola"},"done_reason":"stop","prompt_eval_count":4,"eval_count":1});
        assert_eq!(parse_reply(AiProvider::Ollama, &ollama).unwrap().1.output_tokens, Some(1));
    }

    #[test]
    fn truncated_or_empty_replies_fail() {
        let cut = json!({"choices":[{"message":{"content":"Hal"},"finish_reason":"length"}]});
        assert!(parse_reply(AiProvider::OpenaiCompatible, &cut).unwrap_err().contains("cut off"));
        let cut = json!({"content":[{"type":"text","text":"Hal"}],"stop_reason":"max_tokens"});
        assert!(parse_reply(AiProvider::Anthropic, &cut).is_err());
        let empty = json!({"message":{"content":"  "}});
        assert!(parse_reply(AiProvider::Ollama, &empty).is_err());
        assert!(parse_reply(AiProvider::OpenaiCompatible, &json!({})).is_err());
    }

    #[test]
    fn http_errors_explain_what_to_check() {
        let msg = describe_http_error(AiProvider::OpenaiCompatible, 401, r#"{"error":{"message":"Incorrect API key"}}"#);
        assert!(msg.contains("rejected the API key") && msg.contains("Incorrect API key") && msg.contains("401"));
        let msg = describe_http_error(AiProvider::Ollama, 404, r#"{"error":"model 'x' not found"}"#);
        assert!(msg.contains("model 'x' not found"));
        assert!(describe_http_error(AiProvider::Anthropic, 302, "").contains("redirect"));
    }
}
