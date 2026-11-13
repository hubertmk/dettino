//! Fallback chain runner with hedging.
//!
//! The first provider starts immediately. If it fails, the next starts at once.
//! If it is merely slow (no answer after `hedge_after(i)`), the next one starts in
//! parallel and whichever answers first wins. Every call has its own hard timeout.

use futures::stream::{FuturesUnordered, StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};

use crate::provider::ProviderError;

#[derive(Debug, Clone, PartialEq)]
pub struct ChainAttempt {
    pub index: usize,
    pub latency_ms: u64,
    pub error: Option<ProviderError>,
    /// Started because the previous one was slow, not because it failed.
    pub hedged: bool,
}

#[derive(Debug)]
pub struct ChainOutcome<T> {
    pub result: Result<(usize, T), ProviderError>,
    pub attempts: Vec<ChainAttempt>,
}

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = (usize, Result<T, ProviderError>, u64)> + Send + 'a>>;

/// Run `call(i)` over provider indices `0..n` with fallback and hedging.
/// `hedge_after(i)` is how long to wait on provider `i` before starting the next one
/// in parallel; `None` means the next one starts only if `i` fails (never on slowness).
pub async fn run_chain<'a, T, F, Fut>(
    n: usize,
    hedge_after: impl Fn(usize) -> Option<Duration>,
    timeout: impl Fn(usize) -> Duration,
    call: F,
) -> ChainOutcome<T>
where
    T: Send + 'a,
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<T, ProviderError>> + Send + 'a,
{
    let mut attempts: Vec<ChainAttempt> = Vec::new();
    if n == 0 {
        return ChainOutcome {
            result: Err(ProviderError::NotConfigured("no provider in chain".into())),
            attempts,
        };
    }

    let mut running: FuturesUnordered<BoxFut<'a, T>> = FuturesUnordered::new();
    let launch = |i: usize, running: &mut FuturesUnordered<BoxFut<'a, T>>| {
        let fut = call(i);
        let to = timeout(i);
        running.push(Box::pin(async move {
            let start = Instant::now();
            let r = match tokio::time::timeout(to, fut).await {
                Ok(r) => r,
                Err(_) => Err(ProviderError::Timeout),
            };
            (i, r, start.elapsed().as_millis() as u64)
        }));
    };

    let mut next = 0usize;
    let mut hedged_flags = vec![false; n];
    let mut started: Vec<Option<Instant>> = vec![None; n];
    launch(next, &mut running);
    started[next] = Some(Instant::now());
    next += 1;
    // The attempts still in flight when the chain returns: they lost, and how long
    // they had been running is the only latency sample a slow provider ever gives.
    let losers = |attempts: &mut Vec<ChainAttempt>, started: &[Option<Instant>], hedged: &[bool]| {
        for (i, t) in started.iter().enumerate() {
            if let Some(t) = t {
                if !attempts.iter().any(|a| a.index == i) {
                    attempts.push(ChainAttempt { index: i, latency_ms: t.elapsed().as_millis() as u64, error: Some(ProviderError::Cancelled), hedged: hedged[i] });
                }
            }
        }
    };
    let mut hedge_deadline = hedge_after(0).map(|d| tokio::time::Instant::now() + d);
    let mut last_error = ProviderError::Other("all providers failed".into());

    loop {
        let sleep = async {
            match hedge_deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => futures::future::pending::<()>().await,
            }
        };
        tokio::select! {
            biased;
            item = running.next() => {
                match item {
                    Some((i, Ok(v), ms)) => {
                        attempts.push(ChainAttempt { index: i, latency_ms: ms, error: None, hedged: hedged_flags[i] });
                        // Losers are dropped (cancelled) when `running` goes out of scope.
                        losers(&mut attempts, &started, &hedged_flags);
                        return ChainOutcome { result: Ok((i, v)), attempts };
                    }
                    Some((i, Err(e), ms)) => {
                        attempts.push(ChainAttempt { index: i, latency_ms: ms, error: Some(e.clone()), hedged: hedged_flags[i] });
                        last_error = e;
                        if next < n && running.is_empty() {
                            // Start the next one only when nothing else is in flight: a
                            // hedged attempt that is still running may well answer.
                            launch(next, &mut running);
                            started[next] = Some(Instant::now());
                            hedge_deadline = hedge_after(next).map(|d| tokio::time::Instant::now() + d);
                            next += 1;
                        } else if next < n && !running.is_empty() {
                            // keep waiting for the hedged attempt; its own hedge deadline stands
                        } else if running.is_empty() {
                            return ChainOutcome { result: Err(last_error), attempts };
                        }
                    }
                    None => return ChainOutcome { result: Err(last_error), attempts },
                }
            }
            _ = sleep => {
                if next < n {
                    hedged_flags[next] = true;
                    launch(next, &mut running);
                    started[next] = Some(Instant::now());
                    hedge_deadline = hedge_after(next).map(|d| tokio::time::Instant::now() + d);
                    next += 1;
                } else {
                    hedge_deadline = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    async fn fake(ms: u64, ok: bool, v: &'static str) -> Result<&'static str, ProviderError> {
        tokio::time::sleep(d(ms)).await;
        if ok {
            Ok(v)
        } else {
            Err(ProviderError::Server("500".into()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn primary_wins_when_fast() {
        let out = run_chain(2, |_| Some(d(1000)), |_| d(5000), |i| fake(100, true, ["a", "b"][i])).await;
        assert_eq!(out.result.unwrap(), (0, "a"));
        assert_eq!(out.attempts.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failure_falls_through_immediately() {
        let out = run_chain(
            2,
            |_| Some(d(5000)),
            |_| d(10_000),
            |i| async move { if i == 0 { fake(50, false, "a").await } else { fake(50, true, "b").await } },
        )
        .await;
        assert_eq!(out.result.unwrap(), (1, "b"));
        assert!(out.attempts[0].error.is_some());
        assert!(!out.attempts[1].hedged);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_primary_is_hedged_and_fallback_wins() {
        let out = run_chain(
            2,
            |_| Some(d(1200)),
            |_| d(10_000),
            |i| async move { if i == 0 { fake(5000, true, "a").await } else { fake(200, true, "b").await } },
        )
        .await;
        assert_eq!(out.result.unwrap(), (1, "b"));
        assert!(out.attempts[0].hedged);
        // the slow primary is reported as outpaced, so its slowness is recorded too
        assert_eq!(out.attempts.len(), 2);
        assert_eq!(out.attempts[1].index, 0);
        assert_eq!(out.attempts[1].error, Some(ProviderError::Cancelled));
    }

    #[tokio::test(start_paused = true)]
    async fn no_hedge_means_failover_only() {
        // The slow primary is waited for; the fallback never starts while it runs.
        let out = run_chain(
            2,
            |_| None,
            |_| d(10_000),
            |i| async move { if i == 0 { fake(5000, true, "a").await } else { fake(200, true, "b").await } },
        )
        .await;
        assert_eq!(out.result.unwrap(), (0, "a"));
        assert_eq!(out.attempts.len(), 1);
        // ...but a failure still falls through.
        let out = run_chain(
            2,
            |_| None,
            |_| d(10_000),
            |i| async move { if i == 0 { fake(50, false, "a").await } else { fake(200, true, "b").await } },
        )
        .await;
        assert_eq!(out.result.unwrap(), (1, "b"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_failure_waits_for_the_running_hedge() {
        // primary hedged at 1 s, then fails at 2 s: the third provider is not started
        // while the hedged second one is still running and about to answer (its own
        // hedge window is long enough not to fire first).
        let out = run_chain(
            3,
            |i| Some(if i == 0 { d(1000) } else { d(5000) }),
            |_| d(10_000),
            |i| async move {
                match i {
                    0 => fake(2000, false, "a").await,
                    1 => fake(1500, true, "b").await,
                    _ => fake(10, true, "c").await,
                }
            },
        )
        .await;
        assert_eq!(out.result.unwrap(), (1, "b"));
        assert_eq!(out.attempts.len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_counts_as_failure() {
        let out = run_chain(1, |_| Some(d(100_000)), |_| d(300), |_| fake(5000, true, "a")).await;
        assert_eq!(out.result.unwrap_err(), ProviderError::Timeout);
    }

    #[tokio::test(start_paused = true)]
    async fn all_fail_returns_last_error() {
        let out = run_chain(3, |_| Some(d(100_000)), |_| d(10_000), |_| fake(10, false, "x")).await;
        assert!(out.result.is_err());
        assert_eq!(out.attempts.len(), 3);
    }

    #[tokio::test]
    async fn empty_chain() {
        let out = run_chain(0, |_| Some(d(1)), |_| d(1), |_| fake(1, true, "x")).await;
        assert!(matches!(out.result, Err(ProviderError::NotConfigured(_))));
    }
}
