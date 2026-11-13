//! Cloud transcription: OpenAI `/v1/audio/transcriptions` and any
//! OpenAI-compatible server (Groq, Speaches, faster-whisper-server, LocalAI…).

use app_core::{AsrProvider, AsrRequest, ProviderConfig, ProviderError, ProviderType};
use app_llm::http;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

pub fn build(cfg: &ProviderConfig, key: Option<String>) -> Option<Arc<dyn AsrProvider>> {
    match cfg.ptype {
        ProviderType::OpenaiBatch | ProviderType::OpenaiCompatAsr => Some(Arc::new(OpenAiTranscribe {
            id: cfg.id.clone(),
            base: cfg.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".into()),
            key,
            model: cfg.model.clone(),
            send_prompt: cfg.ptype == ProviderType::OpenaiBatch || cfg.accepts_prompt,
            needs_key: cfg.ptype == ProviderType::OpenaiBatch,
            local: cfg.is_local(),
        })),
        _ => None,
    }
}

pub struct OpenAiTranscribe {
    id: String,
    base: String,
    key: Option<String>,
    model: String,
    send_prompt: bool,
    needs_key: bool,
    local: bool,
}

/// Build the `prompt` field: dictionary terms first, then the text before the caret.
/// whisper-1 allows 224 tokens; ~700 characters is a safe cap for every model.
pub fn build_prompt(hotwords: &[String], tail: Option<&str>) -> Option<String> {
    let mut p = String::new();
    if !hotwords.is_empty() {
        let mut terms = String::new();
        for w in hotwords {
            if terms.len() + w.len() > 400 {
                break;
            }
            if !terms.is_empty() {
                terms.push_str(", ");
            }
            terms.push_str(w);
        }
        p.push_str("Terms that may appear: ");
        p.push_str(&terms);
        p.push_str(". ");
    }
    if let Some(t) = tail {
        let budget = 700usize.saturating_sub(p.len());
        let chars: Vec<char> = t.chars().collect();
        let start = chars.len().saturating_sub(budget);
        p.extend(&chars[start..]);
    }
    let p = p.trim().to_string();
    (!p.is_empty()).then_some(p)
}

#[async_trait]
impl AsrProvider for OpenAiTranscribe {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        self.local
    }
    async fn warmup(&self) -> Result<(), ProviderError> {
        let mut rb = http::client().get(http::join(&self.base, "/models")).timeout(Duration::from_secs(5));
        if let Some(k) = self.key.as_deref().filter(|k| !k.trim().is_empty()) {
            rb = rb.bearer_auth(k.trim());
        }
        let _ = rb.send().await;
        Ok(())
    }
    async fn transcribe(&self, req: &AsrRequest) -> Result<String, ProviderError> {
        let key = self.key.as_deref().map(str::trim).filter(|k| !k.is_empty());
        if self.needs_key && key.is_none() {
            return Err(ProviderError::NotConfigured("API key missing".into()));
        }
        let wav = app_audio::wav::encode(&req.pcm);
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        let mut form = reqwest::multipart::Form::new().part("file", part).text("model", self.model.clone()).text("response_format", "json");
        if !req.language.is_empty() {
            form = form.text("language", req.language.clone());
        }
        if self.send_prompt {
            if let Some(p) = build_prompt(&req.hotwords, req.prompt_tail.as_deref()) {
                form = form.text("prompt", p);
            }
        }
        let mut rb = http::client().post(http::join(&self.base, "/audio/transcriptions")).multipart(form);
        if let Some(k) = key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb.send().await.map_err(http::map_send_error)?;
        if !resp.status().is_success() {
            return Err(http::map_status(resp).await);
        }
        let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
        let body = resp.text().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let text = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => v["text"].as_str().ok_or_else(|| ProviderError::BadResponse("no \"text\" in the reply".into()))?.to_string(),
            // Some servers return the transcript as plain text; an HTML page (a captive
            // portal, a proxy) is not a transcript and must never be pasted.
            Err(_) if content_type.starts_with("text/plain") => body,
            Err(_) => return Err(ProviderError::BadResponse(format!("unexpected {} reply", if content_type.is_empty() { "untyped" } else { &content_type }))),
        };
        Ok(text.trim().to_string())
    }
    fn timeout(&self, audio_ms: u64) -> Duration {
        // upload time grows with length; 4 s after a typical 10 s clip
        Duration::from_millis(4000 + audio_ms / 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Multipart, routing::post, Json, Router};

    #[test]
    fn prompt_building() {
        assert_eq!(build_prompt(&[], None), None);
        let p = build_prompt(&["Lindqvist".into(), "Tauri".into()], Some("Dear Nadia,")).unwrap();
        assert_eq!(p, "Terms that may appear: Lindqvist, Tauri. Dear Nadia,");
        let long = "x".repeat(2000);
        assert!(build_prompt(&[], Some(&long)).unwrap().len() <= 700);
    }

    #[tokio::test]
    async fn sends_multipart_and_parses_text() {
        let app = Router::new().route(
            "/v1/audio/transcriptions",
            post(|mut mp: Multipart| async move {
                let mut fields = Vec::new();
                while let Some(f) = mp.next_field().await.unwrap() {
                    let name = f.name().unwrap().to_string();
                    let data = f.bytes().await.unwrap();
                    fields.push((name, data.len()));
                }
                assert!(fields.iter().any(|(n, l)| n == "file" && *l > 44));
                assert!(fields.iter().any(|(n, _)| n == "prompt"));
                Json(serde_json::json!({"text": " hello world "}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let cfg = ProviderConfig {
            id: "o".into(),
            ptype: ProviderType::OpenaiBatch,
            display_name: "o".into(),
            base_url: Some(format!("http://{addr}/v1")),
            model: "gpt-4o-transcribe".into(),
            models: vec![],
            enabled: true,
            accepts_prompt: false,
            treat_as_local: false,
        };
        let p = build(&cfg, Some("k".into())).unwrap();
        let req = AsrRequest {
            pcm: Arc::new(vec![0; 1600]),
            language: "en".into(),
            hotwords: vec!["Tauri".into()],
            prompt_tail: None,
        };
        assert_eq!(p.transcribe(&req).await.unwrap(), "hello world");
    }
}
