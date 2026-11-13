//! Shared HTTP client and error mapping for every cloud and local HTTP provider.

use app_core::ProviderError;
use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// One client for the whole app: connection pooling and TLS session reuse.
pub fn client() -> reqwest::Client {
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                // Never follow a redirect: reqwest drops Authorization across hosts but
                // not a custom x-api-key header, and provider APIs do not redirect.
                .redirect(reqwest::redirect::Policy::none())
                .pool_idle_timeout(Duration::from_secs(90))
                .user_agent(concat!("dettino/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("http client")
        })
        .clone()
}

pub fn map_send_error(e: reqwest::Error) -> ProviderError {
    if e.is_connect() {
        ProviderError::Offline(short(&e.to_string()))
    } else if e.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Other(short(&e.to_string()))
    }
}

fn short(s: &str) -> String {
    s.chars().take(160).collect()
}

/// Turn a non-success response into a ProviderError (reads the body for context).
pub async fn map_status(resp: reqwest::Response) -> ProviderError {
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let body = resp.text().await.unwrap_or_default();
    status_error(status.as_u16(), retry_after, &body)
}

/// Map a non-success status and its already-read body to a ProviderError.
pub fn status_error(status: u16, retry_after: Option<u64>, body: &str) -> ProviderError {
    tracing::warn!(status, body = %short(body), "provider error");
    match status {
        401 | 403 => ProviderError::KeyInvalid,
        404 => ProviderError::NotConfigured(format!("404: {}", short(body))),
        429 => ProviderError::RateLimited(retry_after),
        400 | 422 => ProviderError::BadResponse(format!("{status}: {}", short(body))),
        s if s >= 500 => ProviderError::Server(format!("{s}")),
        s => ProviderError::Other(format!("{s}: {}", short(body))),
    }
}

/// Join a base URL and a path without doubling or dropping slashes.
pub fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    #[test]
    fn join_paths() {
        assert_eq!(super::join("http://x/v1/", "/chat"), "http://x/v1/chat");
        assert_eq!(super::join("http://x/v1", "chat"), "http://x/v1/chat");
    }
}
