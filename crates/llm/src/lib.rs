//! Cleanup / command LLM providers: Anthropic, OpenAI, Perplexity Agent API,
//! Ollama and any OpenAI-compatible endpoint (LM Studio, llama-server, vLLM, Groq…).

pub mod http;
pub mod ollama;

use app_core::{LlmProvider, LlmRequest, LlmResponse, ProviderConfig, ProviderError, ProviderType};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// Build an LLM provider from its config. `key` is required for cloud providers.
pub fn build(cfg: &ProviderConfig, key: Option<String>) -> Option<Arc<dyn LlmProvider>> {
    let base = cfg.base_url.clone().unwrap_or_default();
    let p: Arc<dyn LlmProvider> = match cfg.ptype {
        ProviderType::Anthropic => Arc::new(Anthropic {
            id: cfg.id.clone(),
            base: if base.is_empty() { "https://api.anthropic.com".into() } else { base },
            key,
            model: cfg.model.clone(),
        }),
        ProviderType::OpenaiChat | ProviderType::OpenaiCompatChat => Arc::new(OpenAiChat {
            id: cfg.id.clone(),
            base: if base.is_empty() { "https://api.openai.com/v1".into() } else { base },
            key,
            model: cfg.model.clone(),
            local: cfg.is_local(),
            json_schema: std::sync::atomic::AtomicBool::new(true),
            reasoning_effort: std::sync::atomic::AtomicBool::new(true),
        }),
        ProviderType::PerplexityAgent => Arc::new(Perplexity {
            id: cfg.id.clone(),
            base: if base.is_empty() { "https://api.perplexity.ai".into() } else { base },
            key,
            models: if cfg.models.is_empty() { vec![cfg.model.clone()] } else { cfg.models.clone() },
        }),
        ProviderType::Ollama => Arc::new(Ollama {
            id: cfg.id.clone(),
            base: if base.is_empty() { "http://localhost:11434".into() } else { base },
            model: cfg.model.clone(),
        }),
        _ => return None,
    };
    Some(p)
}

fn need_key(key: &Option<String>) -> Result<&str, ProviderError> {
    match key.as_deref() {
        Some(k) if !k.trim().is_empty() => Ok(k.trim()),
        _ => Err(ProviderError::NotConfigured("API key missing".into())),
    }
}

/// Extract a JSON object from model text that may be wrapped in prose or code fences.
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let start = t.find('{')?;
    let end = t.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&t[start..=end]).ok()
}

// ---------------------------------------------------------------- Anthropic

pub struct Anthropic {
    id: String,
    base: String,
    key: Option<String>,
    model: String,
}

#[async_trait]
impl LlmProvider for Anthropic {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        false
    }
    async fn warmup(&self) -> Result<(), ProviderError> {
        // Pre-establish TLS so the first dictation does not pay the handshake.
        let key = need_key(&self.key)?;
        let _ = http::client()
            .get(http::join(&self.base, "/v1/models"))
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        Ok(())
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        let key = need_key(&self.key)?;
        // `effort` is accepted by every current model but rejected by the 4.5 generation.
        let mut output_config = if self.model.contains("-4-5") { json!({}) } else { json!({ "effort": "low" }) };
        if let Some(schema) = &req.schema {
            output_config["format"] = json!({ "type": "json_schema", "schema": schema });
        }
        // Low-latency cleanup: thinking off (accepted on Haiku 5.5 at effort ≤ high),
        // no sampling parameters (non-default values are rejected), cached system prefix.
        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "output_config": output_config,
            "system": [{ "type": "text", "text": req.system, "cache_control": { "type": "ephemeral" } }],
            "messages": [{ "role": "user", "content": req.user }],
        });
        // Thinking off where the model allows it; Opus 5.5 / Fable reject any "off"
        // setting, so for those the field is omitted and low effort keeps it short.
        if self.model.contains("haiku") {
            body["thinking"] = json!({ "type": "disabled" });
        } else if self.model.contains("sonnet-5-5") {
            body["thinking"] = json!({ "type": "between_tools" });
        }
        let resp = http::client()
            .post(http::join(&self.base, "/v1/messages"))
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        if v["stop_reason"] == "refusal" {
            return Err(ProviderError::BadResponse("refused".into()));
        }
        let text = v["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.is_empty() {
            return Err(ProviderError::BadResponse("empty response".into()));
        }
        Ok(LlmResponse { text, model: v["model"].as_str().unwrap_or(&self.model).to_string() })
    }
}

// ---------------------------------------------------------------- OpenAI / compatible

pub struct OpenAiChat {
    id: String,
    base: String,
    key: Option<String>,
    model: String,
    local: bool,
    /// Cleared for the session if the server rejects `response_format`.
    json_schema: std::sync::atomic::AtomicBool,
    /// Cleared for the session if the server rejects `reasoning_effort`.
    reasoning_effort: std::sync::atomic::AtomicBool,
}

/// Extra completion tokens for reasoning models, whose hidden reasoning is taken
/// from `max_completion_tokens` before any text is written.
const REASONING_HEADROOM: u32 = 1024;

/// Lowest reasoning effort for OpenAI reasoning models (gpt-5 family, o-series).
/// At the default (medium) effort a short cleanup budget is spent on reasoning and
/// the reply comes back empty.
fn reasoning_effort_for(model: &str) -> Option<&'static str> {
    let m = model.rsplit('/').next().unwrap_or(model).to_ascii_lowercase();
    if let Some(rest) = m.strip_prefix("gpt-5") {
        if m.contains("chat") {
            return None;
        }
        // gpt-5, gpt-5-mini, gpt-5-nano take "minimal"; gpt-5.1 and later take "none".
        return Some(if rest.starts_with('.') { "none" } else { "minimal" });
    }
    if ["o1", "o3", "o4"].iter().any(|p| m.starts_with(p)) {
        return Some("low");
    }
    None
}

impl OpenAiChat {
    async fn send(&self, req: &LlmRequest, with_schema: bool, with_effort: bool) -> Result<reqwest::Response, ProviderError> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": req.system },
                { "role": "user", "content": req.user },
            ],
        });
        let effort = reasoning_effort_for(&self.model);
        if self.local {
            body["max_tokens"] = json!(req.max_tokens);
            body["temperature"] = json!(0);
            body["cache_prompt"] = json!(true);
        } else if effort.is_some() {
            body["max_completion_tokens"] = json!(req.max_tokens + REASONING_HEADROOM);
        } else {
            body["max_completion_tokens"] = json!(req.max_tokens);
        }
        if let Some(effort) = effort.filter(|_| with_effort) {
            body["reasoning_effort"] = json!(effort);
        }
        if with_schema {
            if let Some(schema) = &req.schema {
                body["response_format"] = json!({
                    "type": "json_schema",
                    "json_schema": { "name": "cleanup", "schema": schema, "strict": true }
                });
            }
        }
        let mut rb = http::client().post(http::join(&self.base, "/chat/completions")).json(&body);
        if let Some(k) = self.key.as_deref().filter(|k| !k.trim().is_empty()) {
            rb = rb.bearer_auth(k.trim());
        } else if !self.local {
            return Err(ProviderError::NotConfigured("API key missing".into()));
        }
        rb.send().await.map_err(http::map_send_error)
    }
}

#[async_trait]
impl LlmProvider for OpenAiChat {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        self.local
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        use std::sync::atomic::Ordering;
        let mut schema = self.json_schema.load(Ordering::Relaxed) && req.schema.is_some();
        let mut effort = self.reasoning_effort.load(Ordering::Relaxed) && reasoning_effort_for(&self.model).is_some();
        let resp = loop {
            let resp = self.send(req, schema, effort).await?;
            if resp.status().as_u16() != 400 || !(schema || effort) {
                break resp;
            }
            // Retry once per optional field the server rejects, and remember it for the session.
            let body = resp.text().await.unwrap_or_default();
            if effort && body.contains("reasoning_effort") {
                self.reasoning_effort.store(false, Ordering::Relaxed);
                effort = false;
            } else if schema {
                // Some servers reject response_format: retry without it. That is remembered
                // for the session only when the server said so (or it is a local server,
                // whose messages vary); any other 400 (a prompt that is too long, a bad
                // model id) is not a reason to drop the schema for good.
                if self.local || body.contains("response_format") || body.contains("json_schema") {
                    self.json_schema.store(false, Ordering::Relaxed);
                }
                schema = false;
            } else {
                return Err(http::status_error(400, None, &body));
            }
        };
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let choice = &v["choices"][0];
        let text = choice["message"]["content"].as_str().unwrap_or_default().to_string();
        if text.is_empty() {
            if choice["message"]["refusal"].is_string() {
                return Err(ProviderError::BadResponse("refused".into()));
            }
            if choice["finish_reason"] == "length" {
                tracing::warn!(model = %self.model, usage = %v["usage"], "cleanup reply cut off before any text");
                return Err(ProviderError::BadResponse(format!(
                    "{} used its whole token budget before answering; pick a non-reasoning model",
                    self.model
                )));
            }
            return Err(ProviderError::BadResponse("empty response".into()));
        }
        Ok(LlmResponse { text, model: v["model"].as_str().unwrap_or(&self.model).to_string() })
    }
}

// ---------------------------------------------------------------- Perplexity Agent API

pub struct Perplexity {
    id: String,
    base: String,
    key: Option<String>,
    models: Vec<String>,
}

#[async_trait]
impl LlmProvider for Perplexity {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        false
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        let key = need_key(&self.key)?;
        // JSON is requested in the prompt (not response_format): a new schema can take
        // 10–30 s on its first use, which would blow the cleanup timeout.
        let instructions = if req.schema.is_some() {
            format!("{}\n\nReply with the JSON object only.", req.system)
        } else {
            req.system.clone()
        };
        let models: Vec<&String> = self.models.iter().take(5).collect();
        let body = json!({
            "models": models,
            "instructions": instructions,
            "input": req.user,
            "max_output_tokens": req.max_tokens,
            "max_steps": 1,
        });
        let resp = http::client()
            .post(http::join(&self.base, "/v1/agent"))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let mut text = String::new();
        if let Some(items) = v["output"].as_array() {
            for item in items.iter().filter(|i| i["type"] == "message") {
                if let Some(content) = item["content"].as_array() {
                    for c in content {
                        if let Some(t) = c["text"].as_str() {
                            text.push_str(t);
                        }
                    }
                }
            }
        }
        if text.is_empty() {
            return Err(ProviderError::BadResponse("empty response".into()));
        }
        Ok(LlmResponse { text, model: v["model"].as_str().unwrap_or("perplexity").to_string() })
    }
}

// ---------------------------------------------------------------- Ollama

pub struct Ollama {
    id: String,
    base: String,
    model: String,
}

impl Ollama {
    /// Installed model tags (`GET /api/tags`).
    pub async fn list_models(base: &str) -> Result<Vec<String>, ProviderError> {
        let resp = http::client()
            .get(http::join(base, "/api/tags"))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        Ok(v["models"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }
}

/// Context window every Ollama request asks for. Changing it between requests makes
/// Ollama reload the model, so it stays fixed unless a prompt cannot fit.
const OLLAMA_NUM_CTX: u32 = 4096;

fn ollama_options(num_ctx: u32, num_predict: u32) -> Value {
    json!({ "num_predict": num_predict, "temperature": 0, "num_ctx": num_ctx })
}

/// The usual window, doubled for the rare prompt that would not fit (Ollama truncates
/// the start of the prompt, i.e. the instructions, silently otherwise).
fn ollama_num_ctx(req: &LlmRequest) -> u32 {
    let need = (req.system.chars().count() + req.user.chars().count()) as u32 / 3 + req.max_tokens + 128;
    if need > OLLAMA_NUM_CTX {
        OLLAMA_NUM_CTX * 2
    } else {
        OLLAMA_NUM_CTX
    }
}

#[async_trait]
impl LlmProvider for Ollama {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        true
    }
    async fn warmup(&self) -> Result<(), ProviderError> {
        // Loads the model into memory and keeps it there for 30 minutes. The options
        // must match a real request, or Ollama reloads the model on the first one.
        let body = json!({ "model": self.model, "messages": [], "keep_alive": "30m", "options": ollama_options(OLLAMA_NUM_CTX, 64) });
        let resp = http::client()
            .post(http::join(&self.base, "/api/chat"))
            .json(&body)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        Ok(())
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": req.system },
                { "role": "user", "content": req.user },
            ],
            "stream": false,
            "think": false,
            "keep_alive": "30m",
            "options": ollama_options(ollama_num_ctx(req), req.max_tokens),
        });
        if let Some(schema) = &req.schema {
            body["format"] = schema.clone();
        }
        let resp = http::client()
            .post(http::join(&self.base, "/api/chat"))
            .json(&body)
            .send()
            .await
            .map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let text = v["message"]["content"].as_str().unwrap_or_default().to_string();
        if text.is_empty() {
            return Err(ProviderError::BadResponse("empty response".into()));
        }
        Ok(LlmResponse { text, model: self.model.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};

    #[test]
    fn json_extraction() {
        assert_eq!(extract_json(r#"{"a":1}"#).unwrap()["a"], 1);
        assert_eq!(extract_json("Here:\n```json\n{\"a\":2}\n```").unwrap()["a"], 2);
        assert!(extract_json("no json").is_none());
    }

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://{addr}")
    }

    fn cfg(ptype: ProviderType, base: &str) -> ProviderConfig {
        ProviderConfig {
            id: "t".into(),
            ptype,
            display_name: "t".into(),
            base_url: Some(base.into()),
            model: "m".into(),
            models: vec![],
            enabled: true,
            accepts_prompt: false,
            treat_as_local: false,
        }
    }

    fn req() -> LlmRequest {
        LlmRequest { system: "s".into(), user: "u".into(), schema: Some(json!({"type":"object"})), max_tokens: 64 }
    }

    #[tokio::test]
    async fn anthropic_request_and_parse() {
        let app = Router::new().route(
            "/v1/messages",
            post(|Json(b): Json<Value>| async move {
                assert_eq!(b["output_config"]["format"]["type"], "json_schema");
                assert!(b.get("temperature").is_none());
                assert_eq!(b["system"][0]["cache_control"]["type"], "ephemeral");
                Json(json!({"model":"claude-haiku-5-5","stop_reason":"end_turn","content":[{"type":"text","text":"{\"text\":\"Hi.\"}"}]}))
            }),
        );
        let base = serve(app).await;
        let p = build(&cfg(ProviderType::Anthropic, &base), Some("k".into())).unwrap();
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(extract_json(&r.text).unwrap()["text"], "Hi.");
    }

    #[tokio::test]
    async fn anthropic_401_maps_to_key_invalid() {
        let app = Router::new().route(
            "/v1/messages",
            post(|| async { (axum::http::StatusCode::UNAUTHORIZED, "bad key") }),
        );
        let base = serve(app).await;
        let p = build(&cfg(ProviderType::Anthropic, &base), Some("k".into())).unwrap();
        assert_eq!(p.complete(&req()).await.unwrap_err(), ProviderError::KeyInvalid);
    }

    #[tokio::test]
    async fn missing_key_is_not_configured() {
        let p = build(&cfg(ProviderType::Anthropic, "http://127.0.0.1:9"), None).unwrap();
        assert!(matches!(p.complete(&req()).await, Err(ProviderError::NotConfigured(_))));
    }

    #[tokio::test]
    async fn openai_compat_falls_back_without_schema() {
        let app = Router::new().route(
            "/chat/completions",
            post(|Json(b): Json<Value>| async move {
                if b.get("response_format").is_some() {
                    return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"error":"no"})));
                }
                (axum::http::StatusCode::OK, Json(json!({"model":"m","choices":[{"message":{"content":"{\"text\":\"ok\"}"}}]})))
            }),
        );
        let base = serve(app).await;
        let mut c = cfg(ProviderType::OpenaiCompatChat, &base);
        c.treat_as_local = true;
        let p = build(&c, None).unwrap();
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(extract_json(&r.text).unwrap()["text"], "ok");
    }

    #[test]
    fn reasoning_effort_by_model() {
        assert_eq!(reasoning_effort_for("gpt-5-mini"), Some("minimal"));
        assert_eq!(reasoning_effort_for("gpt-5"), Some("minimal"));
        assert_eq!(reasoning_effort_for("gpt-5.5"), Some("none"));
        assert_eq!(reasoning_effort_for("openai/gpt-5.1-mini"), Some("none"));
        assert_eq!(reasoning_effort_for("gpt-5-chat-latest"), None);
        assert_eq!(reasoning_effort_for("o4-mini"), Some("low"));
        assert_eq!(reasoning_effort_for("gpt-4.1-mini"), None);
    }

    #[tokio::test]
    async fn openai_reasoning_model_gets_low_effort_and_headroom() {
        let app = Router::new().route(
            "/chat/completions",
            post(|Json(b): Json<Value>| async move {
                assert_eq!(b["reasoning_effort"], "minimal");
                assert_eq!(b["max_completion_tokens"], 64 + REASONING_HEADROOM);
                assert!(b.get("temperature").is_none());
                Json(json!({"model":"gpt-5-mini","choices":[{"message":{"content":"{\"text\":\"ok\"}"}}]}))
            }),
        );
        let base = serve(app).await;
        let mut c = cfg(ProviderType::OpenaiChat, &base);
        c.model = "gpt-5-mini".into();
        let p = build(&c, Some("k".into())).unwrap();
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(extract_json(&r.text).unwrap()["text"], "ok");
    }

    #[tokio::test]
    async fn openai_retries_without_rejected_reasoning_effort() {
        let app = Router::new().route(
            "/chat/completions",
            post(|Json(b): Json<Value>| async move {
                if b.get("reasoning_effort").is_some() {
                    let err = json!({"error":{"message":"Unsupported value: 'reasoning_effort'","param":"reasoning_effort"}});
                    return (axum::http::StatusCode::BAD_REQUEST, Json(err));
                }
                assert!(b.get("response_format").is_some(), "schema kept");
                (axum::http::StatusCode::OK, Json(json!({"model":"m","choices":[{"message":{"content":"{\"text\":\"ok\"}"}}]})))
            }),
        );
        let base = serve(app).await;
        let mut c = cfg(ProviderType::OpenaiChat, &base);
        c.model = "gpt-5.9".into();
        let p = build(&c, Some("k".into())).unwrap();
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(extract_json(&r.text).unwrap()["text"], "ok");
    }

    #[tokio::test]
    async fn openai_empty_reply_cut_off_explains_itself() {
        let app = Router::new().route(
            "/chat/completions",
            post(|| async {
                Json(json!({"model":"m","choices":[{"message":{"content":""},"finish_reason":"length"}]}))
            }),
        );
        let base = serve(app).await;
        let p = build(&cfg(ProviderType::OpenaiChat, &base), Some("k".into())).unwrap();
        let err = p.complete(&req()).await.unwrap_err();
        assert!(matches!(err, ProviderError::BadResponse(ref s) if s.contains("token budget")), "{err:?}");
    }

    #[tokio::test]
    async fn perplexity_parses_output_messages() {
        let app = Router::new().route(
            "/v1/agent",
            post(|Json(b): Json<Value>| async move {
                assert!(b["models"].is_array());
                assert!(b["max_output_tokens"].is_number());
                Json(json!({"model":"anthropic/claude-haiku-5-5","output":[
                    {"type":"message","content":[{"type":"output_text","text":"{\"text\":\"P\"}"}]}
                ]}))
            }),
        );
        let base = serve(app).await;
        let p = build(&cfg(ProviderType::PerplexityAgent, &base), Some("k".into())).unwrap();
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(r.model, "anthropic/claude-haiku-5-5");
        assert_eq!(extract_json(&r.text).unwrap()["text"], "P");
    }

    #[tokio::test]
    async fn ollama_chat() {
        let app = Router::new().route(
            "/api/chat",
            post(|Json(b): Json<Value>| async move {
                assert_eq!(b["stream"], false);
                Json(json!({"message":{"content":"{\"text\":\"o\"}"}}))
            }),
        );
        let base = serve(app).await;
        let p = build(&cfg(ProviderType::Ollama, &base), None).unwrap();
        assert!(p.is_local());
        let r = p.complete(&req()).await.unwrap();
        assert_eq!(extract_json(&r.text).unwrap()["text"], "o");
    }

    #[tokio::test]
    async fn offline_maps_to_offline() {
        let p = build(&cfg(ProviderType::Ollama, "http://127.0.0.1:9"), None).unwrap();
        assert!(matches!(p.complete(&req()).await, Err(ProviderError::Offline(_))));
    }
}
