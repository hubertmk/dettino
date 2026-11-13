//! Local per-provider latency statistics (never leave the machine).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

const KEEP: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sample {
    pub ts: u64,
    pub latency_ms: u64,
    pub ok: bool,
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderStats {
    samples: BTreeMap<String, VecDeque<Sample>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StatsSummary {
    pub count: usize,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last_error_ts: Option<u64>,
    pub recent_ms: Vec<u64>,
}

impl ProviderStats {
    pub fn record(&mut self, provider: &str, sample: Sample) {
        let q = self.samples.entry(provider.to_string()).or_default();
        q.push_back(sample);
        while q.len() > KEEP {
            q.pop_front();
        }
    }

    fn percentile(provider_samples: &VecDeque<Sample>, p: f64) -> Option<u64> {
        let mut v: Vec<u64> = provider_samples.iter().filter(|s| s.ok).map(|s| s.latency_ms).collect();
        if v.is_empty() {
            return None;
        }
        v.sort_unstable();
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        Some(v[idx.min(v.len() - 1)])
    }

    pub fn summary(&self, provider: &str) -> StatsSummary {
        let Some(q) = self.samples.get(provider) else { return StatsSummary::default() };
        let last_err = q.iter().rev().find(|s| !s.ok);
        StatsSummary {
            count: q.len(),
            p50_ms: Self::percentile(q, 0.5),
            p95_ms: Self::percentile(q, 0.95),
            last_error: last_err.and_then(|s| s.err.clone()),
            last_error_ts: last_err.map(|s| s.ts),
            recent_ms: q.iter().rev().take(30).filter(|s| s.ok).map(|s| s.latency_ms).collect::<Vec<_>>().into_iter().rev().collect(),
        }
    }

    /// Adaptive hedge delay: max(floor, 1.5 × p95). Without data, `floor × 2`.
    pub fn hedge_after(&self, provider: &str, floor: Duration) -> Duration {
        match self.samples.get(provider).and_then(|q| Self::percentile(q, 0.95)) {
            Some(p95) => floor.max(Duration::from_millis(p95 * 3 / 2)),
            None => floor * 2,
        }
    }

    /// How long to wait for an on-device cleanup model, between 2.5 s and 30 s.
    ///
    /// The measurement is the slower of the warm-up request and the slowest recent
    /// cleanup; a cleanup that timed out counts with the time it was given, so the
    /// budget grows after a timeout instead of failing the same way again. The
    /// measured request produced about 64 tokens, so the time is scaled to this
    /// request's `max_tokens`, then 1.5 × that plus a second. Without any
    /// measurement, 10 s.
    pub fn local_cleanup_timeout(&self, provider: &str, warm_ms: Option<u64>, max_tokens: u32) -> Duration {
        let recent = self.samples.get(provider).and_then(|q| {
            q.iter()
                .rev()
                .take(20)
                .filter(|s| s.ok || s.err.as_deref() == Some("timeout"))
                .map(|s| s.latency_ms)
                .max()
        });
        match warm_ms.max(recent) {
            Some(ms) => {
                let expected = ms as f64 * (1.0 + max_tokens as f64 / 64.0);
                Duration::from_millis(((expected * 1.5) as u64 + 1000).clamp(2500, 30_000))
            }
            None => Duration::from_secs(10),
        }
    }

    /// Recent consecutive failures → caller may demote the provider.
    pub fn consecutive_failures(&self, provider: &str) -> usize {
        self.samples
            .get(provider)
            .map(|q| q.iter().rev().take_while(|s| !s.ok).count())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_and_hedge() {
        let mut s = ProviderStats::default();
        for ms in [100, 200, 300, 400, 1000] {
            s.record("a", Sample { ts: 0, latency_ms: ms, ok: true, err: None });
        }
        let sum = s.summary("a");
        assert_eq!(sum.p50_ms, Some(300));
        assert_eq!(sum.p95_ms, Some(1000));
        assert_eq!(s.hedge_after("a", Duration::from_millis(1200)), Duration::from_millis(1500));
        assert_eq!(s.hedge_after("none", Duration::from_millis(1200)), Duration::from_millis(2400));
    }

    #[test]
    fn local_cleanup_timeout_adapts() {
        let mut s = ProviderStats::default();
        assert_eq!(s.local_cleanup_timeout("a", None, 64), Duration::from_secs(10));
        // a fast GPU model and a short request: the floor
        assert_eq!(s.local_cleanup_timeout("a", Some(240), 64), Duration::from_millis(2500));
        // 600 ms for 64 tokens → 1.2 s expected → 2.8 s budget
        assert_eq!(s.local_cleanup_timeout("a", Some(600), 64), Duration::from_millis(2800));
        // the same model on a long dictation gets proportionally longer
        assert!(s.local_cleanup_timeout("a", Some(600), 450) > Duration::from_secs(8));
        // very slow: capped
        assert_eq!(s.local_cleanup_timeout("a", Some(40_000), 64), Duration::from_secs(30));
        // a slower recent cleanup wins over a fast warm-up; other failures are ignored
        s.record("a", Sample { ts: 1, latency_ms: 4000, ok: true, err: None });
        s.record("a", Sample { ts: 2, latency_ms: 9000, ok: false, err: Some("offline".into()) });
        assert_eq!(s.local_cleanup_timeout("a", Some(600), 64), Duration::from_millis(13_000));
        // a timeout raises the budget instead of being repeated
        s.record("a", Sample { ts: 3, latency_ms: 13_000, ok: false, err: Some("timeout".into()) });
        assert!(s.local_cleanup_timeout("a", Some(600), 64) > Duration::from_millis(13_000));
    }

    #[test]
    fn failures() {
        let mut s = ProviderStats::default();
        s.record("a", Sample { ts: 1, latency_ms: 10, ok: true, err: None });
        s.record("a", Sample { ts: 2, latency_ms: 10, ok: false, err: Some("401".into()) });
        s.record("a", Sample { ts: 3, latency_ms: 10, ok: false, err: Some("timeout".into()) });
        assert_eq!(s.consecutive_failures("a"), 2);
        assert_eq!(s.summary("a").last_error.as_deref(), Some("timeout"));
    }
}
