//! Ollama management beyond chat: pulling a model with progress (`POST /api/pull`)
//! and the server version. The chat provider itself is `crate::Ollama`.

use crate::http;
use app_core::ProviderError;
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

/// One NDJSON line of a streaming pull.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct PullEvent {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Parse one line; blank lines yield `None`, `{"error": …}` lines are errors.
pub fn parse_pull_line(line: &str) -> Result<Option<PullEvent>, ProviderError> {
    let t = line.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let ev: PullEvent = serde_json::from_str(t)
        .map_err(|e| ProviderError::BadResponse(format!("{e}: {}", t.chars().take(120).collect::<String>())))?;
    if let Some(err) = ev.error.as_deref().filter(|s| !s.is_empty()) {
        return Err(ProviderError::Other(err.to_string()));
    }
    Ok(Some(ev))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PullStage {
    #[default]
    Starting,
    Downloading,
    Verifying,
    Ready,
}

impl PullStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
        }
    }
}

/// Folds per-layer pull events into one progress bar. Ollama reports every layer
/// with its own `total`/`completed`; the weights blob dominates, so the largest
/// total seen is the denominator and only that layer moves the bar.
#[derive(Debug, Clone, Default)]
pub struct PullTracker {
    pub total: u64,
    pub done: u64,
    pub stage: PullStage,
    pub status: String,
    last_report: u64,
}

const REPORT_EVERY: u64 = 2_000_000;

impl PullTracker {
    /// Feed one event; returns whether the UI should be updated.
    pub fn feed(&mut self, ev: &PullEvent) -> bool {
        let before = (self.stage, self.done);
        self.status = ev.status.clone();
        let s = ev.status.as_str();
        if s == "success" {
            self.stage = PullStage::Ready;
            if self.total > 0 {
                self.done = self.total;
            }
        } else if s.starts_with("verifying") || s.starts_with("writing") || s.starts_with("removing") {
            self.stage = PullStage::Verifying;
        } else if let Some(total) = ev.total.filter(|t| *t > 0) {
            self.stage = PullStage::Downloading;
            if total >= self.total {
                self.total = total;
                self.done = ev.completed.unwrap_or(0).min(total);
            }
        }
        let stage_changed = before.0 != self.stage;
        let finished = self.total > 0 && self.done == self.total && before.1 != self.done;
        let moved = self.done.saturating_sub(self.last_report) >= REPORT_EVERY;
        if stage_changed || moved || finished {
            self.last_report = self.done;
            return true;
        }
        false
    }
}

/// A pull whose stream goes quiet for this long is treated as broken.
const IDLE_READ: Duration = Duration::from_secs(60);

/// Pull a model, streaming progress events. Returns when Ollama reports success.
pub async fn pull(base: &str, model: &str, mut on_event: impl FnMut(&PullEvent) + Send) -> Result<(), ProviderError> {
    let resp = http::client()
        .post(http::join(base, "/api/pull"))
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(http::map_send_error)?;
    if !resp.status().is_success() {
        return Err(http::map_status(resp).await);
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut succeeded = false;
    while let Some(chunk) = tokio::time::timeout(IDLE_READ, stream.next()).await.map_err(|_| ProviderError::Timeout)? {
        let chunk = chunk.map_err(|e| ProviderError::Offline(format!("download interrupted: {e}")))?;
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            if let Some(ev) = parse_pull_line(std::str::from_utf8(&line).unwrap_or(""))? {
                succeeded |= ev.status == "success";
                on_event(&ev);
            }
        }
    }
    if !buf.is_empty() {
        if let Some(ev) = parse_pull_line(std::str::from_utf8(&buf).unwrap_or(""))? {
            succeeded |= ev.status == "success";
            on_event(&ev);
        }
    }
    if succeeded {
        Ok(())
    } else {
        Err(ProviderError::BadResponse("pull ended without success".into()))
    }
}

/// Server version (`GET /api/version`).
pub async fn version(base: &str) -> Result<String, ProviderError> {
    let resp = http::client()
        .get(http::join(base, "/api/version"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .map_err(http::map_send_error)?;
    if !resp.status().is_success() {
        return Err(http::map_status(resp).await);
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
    Ok(v["version"].as_str().unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};

    const LINES: &str = concat!(
        "{\"status\":\"pulling manifest\"}\n",
        "{\"status\":\"pulling aabb\",\"digest\":\"sha256:aabb\",\"total\":2500000000,\"completed\":1000000000}\n",
        "{\"status\":\"pulling ccdd\",\"digest\":\"sha256:ccdd\",\"total\":1500,\"completed\":1500}\n",
        "{\"status\":\"pulling aabb\",\"digest\":\"sha256:aabb\",\"total\":2500000000,\"completed\":2500000000}\n",
        "{\"status\":\"verifying sha256 digest\"}\n",
        "{\"status\":\"writing manifest\"}\n",
        "{\"status\":\"success\"}\n",
    );

    #[test]
    fn parses_lines_and_errors() {
        assert_eq!(parse_pull_line("   ").unwrap(), None);
        let ev = parse_pull_line("{\"status\":\"pulling x\",\"total\":10,\"completed\":4}").unwrap().unwrap();
        assert_eq!((ev.total, ev.completed), (Some(10), Some(4)));
        assert!(parse_pull_line("{\"error\":\"pull model manifest: file does not exist\"}").is_err());
        assert!(parse_pull_line("not json").is_err());
    }

    #[test]
    fn tracker_follows_the_largest_layer() {
        let mut t = PullTracker::default();
        let mut reports = 0;
        for line in LINES.lines() {
            let ev = parse_pull_line(line).unwrap().unwrap();
            if t.feed(&ev) {
                reports += 1;
            }
            if ev.status.starts_with("pulling ccdd") {
                // the tiny layer must not shrink the bar
                assert_eq!(t.total, 2_500_000_000);
                assert_eq!(t.done, 1_000_000_000);
            }
        }
        assert_eq!(t.stage, PullStage::Ready);
        assert_eq!(t.done, t.total);
        assert!(reports >= 4, "stage changes and byte progress are reported ({reports})");
    }

    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn pull_streams_events_in_order() {
        let app = Router::new().route("/api/pull", post(|| async { LINES }));
        let base = serve(app).await;
        let mut seen = Vec::new();
        pull(&base, "qwen3:4b", |ev| seen.push(ev.status.clone())).await.unwrap();
        assert_eq!(seen.first().map(String::as_str), Some("pulling manifest"));
        assert_eq!(seen.last().map(String::as_str), Some("success"));
        assert_eq!(seen.len(), 7);
    }

    #[tokio::test]
    async fn pull_reports_server_errors() {
        let app = Router::new().route("/api/pull", post(|| async { "{\"error\":\"no such model\"}\n" }));
        let base = serve(app).await;
        let err = pull(&base, "nope", |_| {}).await.unwrap_err();
        assert!(err.to_string().contains("no such model"), "{err}");
    }
}
