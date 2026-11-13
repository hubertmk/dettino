//! Provider traits shared by local and cloud transcription and cleanup adapters.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ProviderError {
    #[error("rejected the key")]
    KeyInvalid,
    #[error("rate limited")]
    RateLimited(Option<u64>),
    #[error("server error: {0}")]
    Server(String),
    #[error("didn't answer in time")]
    Timeout,
    #[error("unreachable: {0}")]
    Offline(String),
    #[error("needs setup: {0}")]
    NotConfigured(String),
    #[error("model missing: {0}")]
    ModelMissing(String),
    #[error("bad response: {0}")]
    BadResponse(String),
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl ProviderError {
    /// Short code for logs and the provider card ("401", "timeout", …).
    pub fn short(&self) -> String {
        match self {
            Self::KeyInvalid => "401".into(),
            Self::RateLimited(_) => "429".into(),
            Self::Server(s) => s.chars().take(24).collect(),
            Self::Timeout => "timeout".into(),
            Self::Offline(_) => "offline".into(),
            Self::NotConfigured(_) => "needs setup".into(),
            Self::ModelMissing(_) => "model missing".into(),
            Self::BadResponse(_) => "bad response".into(),
            Self::Cancelled => "cancelled".into(),
            Self::Other(s) => s.chars().take(24).collect(),
        }
    }
}

/// Audio for a batch transcription: 16 kHz mono i16, already trimmed.
#[derive(Debug, Clone)]
pub struct AsrRequest {
    pub pcm: Arc<Vec<i16>>,
    pub language: String,
    /// Dictionary terms and on-screen proper nouns.
    pub hotwords: Vec<String>,
    /// Text before the caret, used as decoder prompt where supported.
    pub prompt_tail: Option<String>,
}

impl AsrRequest {
    pub fn audio_ms(&self) -> u64 {
        self.pcm.len() as u64 * 1000 / 16_000
    }
}

#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn id(&self) -> &str;
    fn is_local(&self) -> bool;
    /// Load models, open TLS connections. Called at launch and after wake.
    async fn warmup(&self) -> Result<(), ProviderError> {
        Ok(())
    }
    async fn transcribe(&self, req: &AsrRequest) -> Result<String, ProviderError>;
    /// Hard timeout for a request of this length.
    fn timeout(&self, audio_ms: u64) -> Duration {
        Duration::from_millis(4000 + audio_ms / 4)
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    /// JSON schema for structured output, when the provider supports it.
    pub schema: Option<serde_json::Value>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    pub text: String,
    /// Model that actually answered (Perplexity reports which fallback served).
    pub model: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn is_local(&self) -> bool;
    async fn warmup(&self) -> Result<(), ProviderError> {
        Ok(())
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, ProviderError>;
    /// Hard timeout for one request. Cloud: about 1.5 s to connect and start, then the
    /// output at a modest 80 tokens per second, so a long dictation is not cut off at
    /// a fixed 1.5 s. On-device models get their own adaptive budget from the caller.
    fn timeout(&self, req: &LlmRequest) -> Duration {
        if self.is_local() {
            Duration::from_millis(2500)
        } else {
            Duration::from_millis((1500 + req.max_tokens as u64 * 12).min(12_000))
        }
    }
}
