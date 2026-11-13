//! Launch sequence: start Ollama when the cleanup chain needs it, warm the speech and
//! cleanup models, and say so on the pill while that takes a moment ("Brewing the
//! espresso…"). A dictation made meanwhile is recorded as usual; its cleanup waits
//! for Ollama instead of being skipped.

use crate::app::app;
use crate::{ollama, pill, readiness};
use app_core::{ProviderConfig, ProviderType, Settings};
use pill::PillState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What the launch is waiting for; the pill names the first one still pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Ollama,
    Ears,
    Pencil,
}

/// Ollama as the launch sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boot {
    Starting,
    Up,
    Failed,
    NotNeeded,
}

/// Ollama and the speech model warm up side by side; the cleanup model comes last.
struct Progress {
    ollama: bool,
    ears: bool,
    stage: Stage,
    since: Instant,
    /// When the pill started showing the sequence; `None` while it stays quiet.
    shown: Option<Instant>,
}

impl Progress {
    fn pending(&self) -> Stage {
        if self.ollama {
            Stage::Ollama
        } else if self.ears {
            Stage::Ears
        } else {
            Stage::Pencil
        }
    }
}

static PROGRESS: Mutex<Option<Progress>> = Mutex::new(None);
static HEALING: AtomicBool = AtomicBool::new(false);

/// A quick launch (everything already running) stays quiet on the pill.
const REVEAL_AFTER: Duration = Duration::from_millis(800);
/// The first words on the pill, before it names a stage.
const HELLO_FOR: Duration = Duration::from_millis(1200);
/// Ollama's own Startup entry may be booting; give it this long before starting it ourselves.
const OLLAMA_GRACE: Duration = Duration::from_secs(4);
/// At login the whole desktop is still coming up: Ollama's entry can take much longer,
/// and a second start on top of it is what the grace is there to avoid.
const OLLAMA_GRACE_AT_LOGIN: Duration = Duration::from_secs(25);

/// How long to wait for an Ollama that may be starting on its own.
fn grace_for(login_launch: bool) -> Duration {
    if login_launch {
        OLLAMA_GRACE_AT_LOGIN
    } else {
        OLLAMA_GRACE
    }
}
/// Right after a reboot Ollama can take a while to answer.
const OLLAMA_BUDGET: Duration = Duration::from_secs(60);
/// Longest a dictation's cleanup waits for a starting Ollama.
pub const CLEANUP_WAITS: Duration = Duration::from_secs(45);

pub const CLEANUP_OFF: &str = "Ollama didn't start — pasting without AI cleanup";
pub const CLEANUP_MISSING: &str = "Ollama is not installed — pasting without AI cleanup";

/// The words for a stage, `in_stage` after it began and `shown` after the pill
/// started showing the sequence.
fn label_for(stage: Stage, in_stage: Duration, shown: Duration) -> &'static str {
    if shown < HELLO_FOR {
        return "Un momento…";
    }
    match stage {
        Stage::Ollama if in_stage < Duration::from_secs(12) => "Brewing the espresso…",
        Stage::Ollama => "Still brewing — Ollama is waking up…",
        Stage::Ears => "Warming up the ears…",
        Stage::Pencil if in_stage < Duration::from_secs(10) => "Sharpening the pencil…",
        Stage::Pencil => "Still sharpening…",
    }
}

/// The launch line for the pill while the sequence shows; `None` otherwise.
pub fn label() -> Option<String> {
    let g = PROGRESS.lock().unwrap();
    let p = g.as_ref()?;
    let shown = p.shown?;
    Some(label_for(p.stage, p.since.elapsed(), shown.elapsed()).into())
}

/// Ollama as the launch sees it, read off the readiness of the primary Ollama.
pub fn boot() -> Boot {
    boot_of(&readiness::current().ollama)
}

fn boot_of(stage: &readiness::Stage) -> Boot {
    match stage {
        readiness::Stage::NotNeeded => Boot::NotNeeded,
        readiness::Stage::Warming => Boot::Starting,
        readiness::Stage::Ready => Boot::Up,
        readiness::Stage::Missing | readiness::Stage::Failed { .. } => Boot::Failed,
    }
}

/// Show the current launch line, unless a dictation owns the pill right now.
fn refresh_pill() {
    let cur = pill::current();
    if matches!(cur.kind.as_str(), "idle" | "starting") && !pill::capturing() {
        // `pill::set` turns idle into the launch line while there is one.
        pill::set(pill::state("idle"));
    }
}

fn update(f: impl FnOnce(&mut Progress)) {
    let shown = {
        let mut g = PROGRESS.lock().unwrap();
        let Some(p) = g.as_mut() else { return };
        f(p);
        let stage = p.pending();
        if stage != p.stage {
            p.stage = stage;
            p.since = Instant::now();
        }
        p.shown.is_some()
    };
    if shown {
        refresh_pill();
    }
}

/// The Ollama base Dettino should start, if any: Ollama is in the cleanup chain,
/// runs on this computer, and starting it is allowed.
fn ollama_to_start(s: &Settings, chain: &[ProviderConfig]) -> Option<String> {
    if !s.start_ollama {
        return None;
    }
    chain.iter().find(|p| p.ptype == ProviderType::Ollama).map(base_of).filter(|b| ollama::is_local_base(b))
}

fn base_of(cfg: &ProviderConfig) -> String {
    cfg.base_url.clone().unwrap_or_else(|| ollama::DEFAULT_BASE.into())
}

/// Start the launch sequence. During onboarding it runs without pill words: the
/// setup window has the screen.
pub fn run() {
    let a = app();
    let s = a.settings();
    let chain = a.chain(&s.llm_chain);
    let base = ollama_to_start(&s, &chain);
    let quiet = !s.onboarding_done;
    readiness::begin(&s);
    // The launch is the first warm pass; anything asked for meanwhile runs after it.
    let _ = crate::warm_begin();
    *PROGRESS.lock().unwrap() =
        Some(Progress { ollama: base.is_some(), ears: true, stage: if base.is_some() { Stage::Ollama } else { Stage::Ears }, since: Instant::now(), shown: None });

    if !quiet {
        tauri::async_runtime::spawn(async {
            tokio::time::sleep(REVEAL_AFTER).await;
            {
                let mut g = PROGRESS.lock().unwrap();
                let Some(p) = g.as_mut() else { return };
                p.shown = Some(Instant::now());
            }
            // Re-show once a second so the words follow the clock ("Still brewing…").
            while PROGRESS.lock().unwrap().is_some() {
                refresh_pill();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    tauri::async_runtime::spawn(async move {
        let t0 = Instant::now();
        let ollama_up = async {
            let base = base.as_deref()?;
            let login = app_platform::launched_at_login();
            let r = ollama::ensure_running(base, grace_for(login), OLLAMA_BUDGET).await;
            readiness::set_ollama(readiness::stage_after_start(&r, ollama::installed()));
            update(|p| p.ollama = false);
            Some(r)
        };
        let ears = async {
            crate::warm_asr().await;
            update(|p| p.ears = false);
        };
        let (ollama_r, ()) = tokio::join!(ollama_up, ears);
        crate::auto_pick_ollama_model().await;
        let a = app();
        let s = a.settings();
        if let Some(cfg) = a.chain(&s.llm_chain).first().cloned() {
            // A cleanup model on an Ollama that did not start would only time out.
            if cfg.ptype == ProviderType::Ollama && ollama_r.as_ref().is_some_and(|r| r.is_err()) {
                readiness::set_cleanup(readiness::Stage::Failed { error: "Ollama isn't running".into() });
            } else {
                crate::warm_llm(&cfg).await;
            }
        }
        let shown = PROGRESS.lock().unwrap().take().is_some_and(|p| p.shown.is_some());
        let ms = t0.elapsed().as_millis() as u64;
        match ollama_r {
            Some(Err(e)) => {
                tracing::warn!(ms, "startup: Ollama unavailable: {e}");
                if quiet {
                    // The setup window has the screen, and it installs Ollama itself.
                } else if !ollama::installed() {
                    pill::set_badge(Some("cleanup_off"), Some(CLEANUP_MISSING));
                } else {
                    pill::set_badge(Some("cleanup_off"), Some(CLEANUP_OFF));
                    if shown {
                        pill::flash(PillState { kind: "notice".into(), label: Some(CLEANUP_OFF.into()), ..Default::default() }, 4000);
                    }
                }
            }
            r => {
                tracing::info!(ms, shown, ollama_started = matches!(r, Some(Ok(true))), "startup done");
                if shown {
                    pill::flash(PillState { kind: "ready".into(), label: Some("Pronto!".into()), ..Default::default() }, 2500);
                }
            }
        }
        if !shown && pill::current().kind == "starting" {
            pill::set(pill::state("idle"));
        }
        if crate::warm_finish() {
            crate::warm_continue();
        }
        watch_ollama();
    });
}

/// What the Ollama watch does on a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watch {
    /// Installed and not answering: start it (again).
    Start,
    /// Not installed, remote, or starting is not allowed: only look whether it answers now.
    Probe,
    /// Up, starting, or not part of the mode.
    Idle,
}

fn watch_action(stage: &readiness::Stage, start_ollama: bool, local: bool) -> Watch {
    match stage {
        readiness::Stage::Failed { .. } if start_ollama && local => Watch::Start,
        readiness::Stage::Failed { .. } | readiness::Stage::Missing => Watch::Probe,
        _ => Watch::Idle,
    }
}

/// Pause before start attempt `n`: 30 s, doubling, at most five minutes.
fn retry_delay(n: u32) -> Duration {
    Duration::from_secs((30u64 << n.min(4)).min(300))
}

/// How often the watch looks while nothing is wrong or while it can only probe.
const WATCH_EVERY: Duration = Duration::from_secs(30);

static WATCHING: AtomicBool = AtomicBool::new(false);

/// After the launch: an Ollama that did not start is started again with a growing
/// pause between attempts, and one that is missing (or that Dettino may not start) is
/// probed, so an Ollama installed or started by hand is noticed within half a minute
/// instead of at the next dictation or the next 20-minute warm-up.
fn watch_ollama() {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async {
        let mut attempt = 0u32;
        loop {
            let a = app();
            let s = a.settings();
            let Some(cfg) = readiness::primary_ollama(&s) else {
                attempt = 0;
                tokio::time::sleep(WATCH_EVERY).await;
                continue;
            };
            let base = base_of(&cfg);
            let local = ollama::is_local_base(&base);
            let mut wait = WATCH_EVERY;
            match watch_action(&readiness::current().ollama, s.start_ollama, local) {
                Watch::Idle => attempt = 0,
                Watch::Probe => {
                    let st = ollama::status(&base).await;
                    readiness::observe_ollama(&base, &st, Some(cfg.model.as_str()), true);
                }
                Watch::Start => {
                    if HEALING.load(Ordering::SeqCst) {
                        tokio::time::sleep(WATCH_EVERY).await;
                        continue;
                    }
                    tracing::info!(attempt, "Ollama is not answering; starting it again");
                    readiness::set_ollama(readiness::Stage::Warming);
                    match ollama::ensure_running(&base, Duration::ZERO, Duration::from_secs(30)).await {
                        Ok(started) => {
                            tracing::info!(started, "Ollama answers again");
                            readiness::set_ollama(readiness::Stage::Ready);
                            crate::warm_llm(&cfg).await;
                            attempt = 0;
                        }
                        Err(e) => {
                            tracing::warn!(attempt, "could not start Ollama: {e}");
                            readiness::set_ollama(readiness::stage_after_start(&Err::<bool, String>(e), ollama::installed()));
                            wait = retry_delay(attempt);
                            attempt = attempt.saturating_add(1);
                        }
                    }
                }
            }
            tokio::time::sleep(wait).await;
        }
    });
}

/// Wait while Ollama is still starting, at most `max`.
pub async fn wait_for_ollama(max: Duration) {
    let t0 = Instant::now();
    while boot() == Boot::Starting && t0.elapsed() < max {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Ollama stopped answering (quit, crashed): start it again in the background so
/// the next dictation is cleaned up.
pub fn heal(cfg: &ProviderConfig) {
    let base = base_of(cfg);
    if !app().settings().start_ollama || !ollama::is_local_base(&base) || boot() == Boot::Starting {
        return;
    }
    if HEALING.swap(true, Ordering::SeqCst) {
        return;
    }
    readiness::set_ollama(readiness::Stage::Warming);
    let cfg = cfg.clone();
    tauri::async_runtime::spawn(async move {
        match ollama::ensure_running(&base, Duration::ZERO, Duration::from_secs(30)).await {
            Ok(started) => {
                tracing::info!(started, "Ollama answers again");
                // Readiness clears the "didn't start" badge when Ollama comes up.
                readiness::set_ollama(readiness::Stage::Ready);
                crate::warm_llm(&cfg).await;
            }
            Err(e) => {
                tracing::warn!("could not restart Ollama: {e}");
                readiness::set_ollama(readiness::stage_after_start(&Err::<bool, String>(e), ollama::installed()));
            }
        }
        HEALING.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn words_follow_the_stage_and_the_clock() {
        assert_eq!(label_for(Stage::Ollama, S(0), Duration::ZERO), "Un momento…");
        assert_eq!(label_for(Stage::Ollama, S(2), S(2)), "Brewing the espresso…");
        assert_eq!(label_for(Stage::Ollama, S(15), S(15)), "Still brewing — Ollama is waking up…");
        assert_eq!(label_for(Stage::Ears, S(30), S(30)), "Warming up the ears…");
        assert_eq!(label_for(Stage::Pencil, S(1), S(5)), "Sharpening the pencil…");
        assert_eq!(label_for(Stage::Pencil, S(11), S(20)), "Still sharpening…");
    }

    #[test]
    fn boot_follows_the_ollama_stage() {
        use crate::readiness::Stage as R;
        assert_eq!(boot_of(&R::NotNeeded), Boot::NotNeeded);
        assert_eq!(boot_of(&R::Warming), Boot::Starting);
        assert_eq!(boot_of(&R::Ready), Boot::Up);
        assert_eq!(boot_of(&R::Missing), Boot::Failed);
        assert_eq!(boot_of(&R::Failed { error: "x".into() }), Boot::Failed);
    }

    #[test]
    fn the_first_pending_stage_shows() {
        let mut p = Progress { ollama: true, ears: true, stage: Stage::Ollama, since: Instant::now(), shown: None };
        assert_eq!(p.pending(), Stage::Ollama);
        p.ollama = false;
        assert_eq!(p.pending(), Stage::Ears);
        p.ears = false;
        assert_eq!(p.pending(), Stage::Pencil);
        p.ollama = true;
        assert_eq!(p.pending(), Stage::Ollama);
    }

    #[test]
    fn a_login_launch_gives_ollamas_own_startup_entry_more_time() {
        assert_eq!(grace_for(false), OLLAMA_GRACE);
        assert!(grace_for(true) >= Duration::from_secs(20), "{:?}", grace_for(true));
    }

    #[test]
    fn retries_back_off_and_cap() {
        let d: Vec<u64> = (0..7).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(d[0], 30);
        assert!(d.windows(2).all(|w| w[0] <= w[1]), "{d:?}");
        assert_eq!(d[6], 300, "capped at five minutes");
        assert_eq!(retry_delay(100), retry_delay(6));
    }

    #[test]
    fn the_watch_starts_a_failed_ollama_and_only_probes_a_missing_or_foreign_one() {
        use crate::readiness::Stage as R;
        let failed = R::failed("no answer");
        assert_eq!(watch_action(&failed, true, true), Watch::Start, "installed, failed, we may start it");
        assert_eq!(watch_action(&failed, false, true), Watch::Probe, "start_ollama is off: only look");
        assert_eq!(watch_action(&failed, true, false), Watch::Probe, "a remote Ollama is never started");
        assert_eq!(watch_action(&R::Missing, true, true), Watch::Probe, "not installed: it may appear");
        assert_eq!(watch_action(&R::Ready, true, true), Watch::Idle);
        assert_eq!(watch_action(&R::Warming, true, true), Watch::Idle, "a start is already under way");
        assert_eq!(watch_action(&R::NotNeeded, true, true), Watch::Idle);
    }

    #[test]
    fn only_a_local_ollama_in_the_chain_is_started() {
        let mut s = Settings::default();
        let ollama = s.providers.iter().find(|p| p.ptype == ProviderType::Ollama).cloned().unwrap();
        let cloud = s.providers.iter().find(|p| p.ptype != ProviderType::Ollama && p.kind() == ollama.kind()).cloned().unwrap();
        assert_eq!(ollama_to_start(&s, &[cloud.clone(), ollama.clone()]).as_deref(), Some(ollama::DEFAULT_BASE));
        assert_eq!(ollama_to_start(&s, std::slice::from_ref(&cloud)), None);
        let mut remote = ollama.clone();
        remote.base_url = Some("http://192.0.2.20:11434".into());
        assert_eq!(ollama_to_start(&s, &[remote]), None);
        s.start_ollama = false;
        assert_eq!(ollama_to_start(&s, &[ollama]), None);
    }
}
