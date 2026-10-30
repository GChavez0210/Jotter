//! Embedding requests for "Ask your vault". Only the OpenAI-compatible and Ollama endpoints
//! offer embeddings; Anthropic has no embeddings API. The requests reuse the endpoint and the
//! origin-bound key saved for the chat provider.

use serde_json::{json, Value};
use url::Url;

use super::config::AiProvider;
use super::provider::{describe_http_error, describe_transport_error};

pub struct EmbedCall<'a> {
    pub provider: AiProvider,
    pub base_url: &'a Url,
    pub model: &'a str,
    pub api_key: Option<&'a str>,
    pub inputs: &'a [String],
}

pub fn supports_embeddings(provider: AiProvider) -> bool {
    provider != AiProvider::Anthropic
}

pub fn request_url(provider: AiProvider, base_url: &Url) -> Url {
    let suffix = match provider {
        AiProvider::Ollama => "api/embed",
        _ => "embeddings",
    };
    let mut url = base_url.clone();
    let path = format!("{}/{suffix}", url.path().trim_end_matches('/'));
    url.set_path(&path);
    url
}

pub fn request_body(call: &EmbedCall<'_>) -> Value {
    json!({ "model": call.model, "input": call.inputs })
}

/// One vector per input, in input order.
pub fn parse_reply(provider: AiProvider, body: &Value, expected: usize) -> Result<Vec<Vec<f32>>, String> {
    let unreadable = || format!("{} returned an embeddings reply Jotter cannot read.", provider.label());
    let vector = |value: &Value| -> Option<Vec<f32>> {
        value.as_array()?.iter().map(|n| n.as_f64().map(|f| f as f32)).collect()
    };
    let vectors: Vec<Vec<f32>> = match provider {
        AiProvider::Ollama => body
            .get("embeddings")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?
            .iter()
            .map(|v| vector(v).ok_or_else(unreadable))
            .collect::<Result<_, _>>()?,
        _ => {
            let mut rows: Vec<(u64, Vec<f32>)> = body
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(unreadable)?
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let index = row.get("index").and_then(Value::as_u64).unwrap_or(i as u64);
                    vector(row.get("embedding")?).map(|v| (index, v))
                })
                .collect::<Option<_>>()
                .ok_or_else(unreadable)?;
            rows.sort_by_key(|(index, _)| *index);
            rows.into_iter().map(|(_, v)| v).collect()
        }
    };
    if vectors.len() != expected || vectors.iter().any(|v| v.is_empty()) {
        return Err(format!(
            "{} returned {} vectors for {expected} passages. Check the embedding model name in Settings > AI.",
            provider.label(),
            vectors.len()
        ));
    }
    Ok(vectors)
}

pub async fn send_embeddings(client: &reqwest::Client, call: &EmbedCall<'_>) -> Result<Vec<Vec<f32>>, String> {
    let mut request = client.post(request_url(call.provider, call.base_url)).json(&request_body(call));
    if let Some(key) = call.api_key {
        request = request.bearer_auth(key);
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
        .map_err(|_| format!("{} returned an embeddings reply Jotter cannot read.", call.provider.label()))?;
    parse_reply(call.provider, &json, call.inputs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_append_each_providers_path() {
        let base = Url::parse("https://api.openai.com/v1").unwrap();
        assert_eq!(request_url(AiProvider::OpenaiCompatible, &base).as_str(), "https://api.openai.com/v1/embeddings");
        let base = Url::parse("http://localhost:11434").unwrap();
        assert_eq!(request_url(AiProvider::Ollama, &base).as_str(), "http://localhost:11434/api/embed");
    }

    #[test]
    fn openai_rows_return_in_input_order() {
        let body = json!({"data":[{"index":1,"embedding":[0.0,1.0]},{"index":0,"embedding":[1.0,0.0]}]});
        let vectors = parse_reply(AiProvider::OpenaiCompatible, &body, 2).unwrap();
        assert_eq!(vectors, vec![vec![1.0, 0.0], vec![0.0, 1.0]]);
    }

    #[test]
    fn ollama_embeddings_parse() {
        let body = json!({"embeddings":[[0.5,0.5]]});
        assert_eq!(parse_reply(AiProvider::Ollama, &body, 1).unwrap(), vec![vec![0.5, 0.5]]);
    }

    #[test]
    fn a_wrong_count_or_empty_vector_is_an_error() {
        let body = json!({"data":[{"embedding":[1.0]}]});
        assert!(parse_reply(AiProvider::OpenaiCompatible, &body, 2).unwrap_err().contains("embedding model"));
        assert!(parse_reply(AiProvider::Ollama, &json!({"embeddings":[[]]}), 1).is_err());
        assert!(parse_reply(AiProvider::Ollama, &json!({}), 1).is_err());
    }

    #[test]
    fn anthropic_has_no_embeddings() {
        assert!(!supports_embeddings(AiProvider::Anthropic));
        assert!(supports_embeddings(AiProvider::Ollama));
    }
}
