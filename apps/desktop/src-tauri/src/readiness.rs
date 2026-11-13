//! What is loaded and answering right now: the speech model, Ollama and the cleanup
//! model. Warm-ups, starts, installs and pulls report here; the UI gets a `readiness`
//! event on every change and the current value in every snapshot, so the onboarding
//! says "installed and running" only when that is true, and Home can name what is
//! missing.

use crate::app::{app, try_app};
use crate::ollama::{self, OllamaStatus};
use crate::pill;
use app_core::{CleanupLevel, ProviderConfig, ProviderType, Settings};
use serde::Serialize;
use std::sync::Mutex;
use tauri::Emitter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Stage {
    /// Not part of the current mode (Ollama in Cloud mode, cleanup when it is off).
    NotNeeded,
    /// The files, the program or the model are not there.
    Missing,
    /// Loading, starting, or the warm request is running.
    Warming,
    /// Loaded in memory / the API answers / one real completion succeeded.
    Ready,
    /// Tried this session and it did not work.
    Failed { error: String },
}

impl Stage {
    pub fn is_ok(&self) -> bool {
        matches!(self, Stage::Ready | Stage::NotNeeded)
    }

    pub(crate) fn failed(e: impl Into<String>) -> Self {
        Stage::Failed { error: e.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Readiness {
    /// The first transcription provider.
    pub speech: Stage,
    /// The Ollama server, when the first cleanup provider is Ollama.
    pub ollama: Stage,
    /// The first cleanup provider: model present and one warm completion done.
    pub cleanup: Stage,
    /// Every stage is Ready or NotNeeded.
    pub all: bool,
}

/// What the stages were derived for; a re-warm of the same provider keeps Ready.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Targets {
    speech: String,
    ollama: String,
    cleanup: String,
}

struct Inner {
    r: Readiness,
    t: Targets,
}

impl Default for Inner {
    fn default() -> Self {
        // Until the first launch pass says otherwise, everything is "still checking".
        Self { r: Readiness { speech: Stage::Warming, ollama: Stage::Warming, cleanup: Stage::Warming, all: false }, t: Targets::default() }
    }
}

static STATE: Mutex<Option<Inner>> = Mutex::new(None);
/// The last Ollama probe per base, so the snapshot can say whether Ollama is usable
/// without asking it again.
static OLLAMA_SEEN: Mutex<Option<(String, OllamaStatus)>> = Mutex::new(None);

pub fn current() -> Readiness {
    let mut g = STATE.lock().unwrap();
    g.get_or_insert_with(Inner::default).r.clone()
}

fn all_ready(r: &Readiness) -> bool {
    r.speech.is_ok() && r.ollama.is_ok() && r.cleanup.is_ok()
}

/// Apply a change; on a difference tell the UI, and drop the badge that says Ollama
/// did not start once it answers.
fn update(f: impl FnOnce(&mut Inner)) {
    let (r, ollama_came_up) = {
        let mut g = STATE.lock().unwrap();
        let i = g.get_or_insert_with(Inner::default);
        let before = i.r.clone();
        f(i);
        i.r.all = all_ready(&i.r);
        if before == i.r {
            return;
        }
        (i.r.clone(), before.ollama != Stage::Ready && i.r.ollama == Stage::Ready)
    };
    if ollama_came_up {
        pill::clear_badge("cleanup_off");
    }
    if let Some(a) = try_app() {
        let _ = a.handle.emit("readiness", &r);
    }
}

fn base_of(cfg: &ProviderConfig) -> String {
    cfg.base_url.clone().unwrap_or_else(|| ollama::DEFAULT_BASE.into())
}

/// The first cleanup provider, when it is Ollama.
pub fn primary_ollama(s: &Settings) -> Option<ProviderConfig> {
    s.llm_chain.first().and_then(|id| s.provider(id)).filter(|p| p.ptype == ProviderType::Ollama).cloned()
}

/// The configured Ollama model, when `base` is the primary Ollama's base.
pub fn primary_model_at(s: &Settings, base: &str) -> Option<String> {
    primary_ollama(s).filter(|p| base_of(p) == base).map(|p| p.model)
}

/// The stages a mode starts from, before anything is warmed: what needs loading is
/// Warming, what is not there is Missing, cloud providers are Ready once keyed.
fn derive(s: &Settings, has_key: impl Fn(&str) -> bool, installed: impl Fn(&ProviderConfig) -> bool) -> (Readiness, Targets) {
    let mut t = Targets::default();
    let speech = match s.asr_chain.first().and_then(|id| s.provider(id)) {
        None => Stage::Missing,
        Some(p) => {
            t.speech = format!("{}:{}", p.id, p.model);
            if p.is_local() {
                if installed(p) {
                    Stage::Warming
                } else {
                    Stage::Missing
                }
            } else if has_key(&p.id) {
                Stage::Ready
            } else {
                Stage::Missing
            }
        }
    };
    let cleanup_on = s.cleanup_level != CleanupLevel::Off;
    let primary = s.llm_chain.first().and_then(|id| s.provider(id)).filter(|_| cleanup_on);
    let ollama = match primary {
        Some(p) if p.ptype == ProviderType::Ollama => {
            t.ollama = base_of(p);
            Stage::Warming
        }
        _ => Stage::NotNeeded,
    };
    let cleanup = match primary {
        None => Stage::NotNeeded,
        Some(p) => {
            t.cleanup = format!("{}:{}", p.id, p.model);
            if p.is_local() {
                Stage::Warming
            } else if has_key(&p.id) {
                Stage::Ready
            } else {
                Stage::Missing
            }
        }
    };
    let mut r = Readiness { speech, ollama, cleanup, all: false };
    r.all = all_ready(&r);
    (r, t)
}

/// Start of a warm pass: derive the stages for the current settings. A provider that
/// is already Ready and unchanged stays Ready, so a re-warm does not flicker.
pub fn begin(s: &Settings) {
    let a = app();
    let (next, t) = derive(s, |id| a.has_key(id), |p| a.local_installed(p));
    update(|i| {
        let keep = |cur: &Stage, nxt: &Stage, same: bool| if same && *cur == Stage::Ready && *nxt == Stage::Warming { Stage::Ready } else { nxt.clone() };
        i.r.speech = keep(&i.r.speech, &next.speech, i.t.speech == t.speech);
        i.r.ollama = keep(&i.r.ollama, &next.ollama, i.t.ollama == t.ollama);
        i.r.cleanup = keep(&i.r.cleanup, &next.cleanup, i.t.cleanup == t.cleanup);
        i.t = t;
    });
}

pub fn set_speech(s: Stage) {
    update(|i| i.r.speech = s);
}

/// Ignored while Ollama is not part of the mode.
pub fn set_ollama(s: Stage) {
    update(|i| {
        if i.r.ollama != Stage::NotNeeded {
            i.r.ollama = s;
        }
    });
}

/// Ignored while cleanup is off.
pub fn set_cleanup(s: Stage) {
    update(|i| {
        if i.r.cleanup != Stage::NotNeeded {
            i.r.cleanup = s;
        }
    });
}

/// What a probe says about Ollama: answering, not installed, or installed but silent.
pub fn stage_for_ollama(st: &OllamaStatus, local: bool, base: &str) -> Stage {
    if st.reachable {
        Stage::Ready
    } else if local && !st.installed {
        Stage::Missing
    } else if local {
        Stage::failed("Ollama is installed but not answering")
    } else {
        Stage::failed(format!("Ollama at {base} is not answering"))
    }
}

/// What a start attempt says: up, not installed, or failed with the reason.
pub fn stage_after_start(r: &Result<bool, String>, installed: bool) -> Stage {
    match r {
        Ok(_) => Stage::Ready,
        Err(_) if !installed => Stage::Missing,
        Err(e) => Stage::failed(e.clone()),
    }
}

/// Record a probe. For the primary Ollama it sets the Ollama stage, and the cleanup
/// stage to Missing when the server answers but the model is not pulled. With `kick`,
/// an Ollama that has just come up (started by hand, say) gets its model warmed; a
/// probe made inside a warm pass passes `false`, that pass warms it anyway.
pub fn observe_ollama(base: &str, st: &OllamaStatus, model: Option<&str>, kick: bool) {
    *OLLAMA_SEEN.lock().unwrap() = Some((base.to_string(), st.clone()));
    let local = ollama::is_local_base(base);
    let mut came_up = false;
    update(|i| {
        if i.r.ollama == Stage::NotNeeded || i.t.ollama != base {
            return;
        }
        let next = stage_for_ollama(st, local, base);
        came_up = i.r.ollama != Stage::Ready && next == Stage::Ready;
        i.r.ollama = next;
        if let Some(m) = model {
            let has = st.models.iter().any(|x| x == m);
            if st.reachable && !has {
                i.r.cleanup = Stage::Missing;
            } else if st.reachable && has && i.r.cleanup == Stage::Missing {
                i.r.cleanup = Stage::Warming;
            }
        }
    });
    if kick && came_up && !current().cleanup.is_ok() {
        crate::warmup();
    }
}

/// Whether the snapshot may call this Ollama provider ready: the last probe of its
/// base answered and listed its model.
pub fn ollama_usable(cfg: &ProviderConfig) -> bool {
    let base = base_of(cfg);
    OLLAMA_SEEN.lock().unwrap().as_ref().is_some_and(|(b, st)| *b == base && st.reachable && st.models.contains(&cfg.model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Preset;

    fn st(installed: bool, reachable: bool, models: &[&str]) -> OllamaStatus {
        OllamaStatus { installed, reachable, models: models.iter().map(|m| m.to_string()).collect(), version: None, can_install: true }
    }

    #[test]
    fn private_mode_starts_with_everything_to_warm() {
        let s = Settings::default();
        let (r, t) = derive(&s, |_| false, |_| false);
        assert_eq!((r.speech.clone(), r.ollama.clone(), r.cleanup.clone(), r.all), (Stage::Missing, Stage::Warming, Stage::Warming, false));
        assert_eq!(t.ollama, ollama::DEFAULT_BASE);
        let (r, _) = derive(&s, |_| false, |_| true);
        assert_eq!(r.speech, Stage::Warming, "an installed speech model loads");
    }

    #[test]
    fn fast_mode_is_ready_once_keyed() {
        let mut s = Settings::default();
        s.apply_preset(Preset::Fast);
        let (r, _) = derive(&s, |_| false, |_| true);
        assert_eq!((r.speech.clone(), r.ollama.clone(), r.cleanup.clone()), (Stage::Missing, Stage::NotNeeded, Stage::Missing));
        let (r, _) = derive(&s, |_| true, |_| true);
        assert!(r.all, "{r:?}");
    }

    #[test]
    fn cleanup_off_needs_neither_ollama_nor_a_model() {
        let s = Settings { cleanup_level: CleanupLevel::Off, ..Default::default() };
        let (r, _) = derive(&s, |_| false, |_| true);
        assert_eq!((r.ollama.clone(), r.cleanup.clone()), (Stage::NotNeeded, Stage::NotNeeded));
        assert!(!r.all, "the speech model still has to load");
    }

    #[test]
    fn probes_and_starts_map_to_stages() {
        let b = ollama::DEFAULT_BASE;
        assert_eq!(stage_for_ollama(&st(true, true, &[]), true, b), Stage::Ready);
        assert_eq!(stage_for_ollama(&st(false, false, &[]), true, b), Stage::Missing);
        assert!(matches!(stage_for_ollama(&st(true, false, &[]), true, b), Stage::Failed { .. }));
        assert!(matches!(stage_for_ollama(&st(false, false, &[]), false, "http://box:11434"), Stage::Failed { .. }), "a remote Ollama is never 'not installed'");
        assert_eq!(stage_after_start(&Ok(true), true), Stage::Ready);
        assert_eq!(stage_after_start(&Err("Ollama is not installed.".into()), false), Stage::Missing);
        assert_eq!(stage_after_start(&Err("no answer".into()), true), Stage::failed("no answer"));
    }

    #[test]
    fn all_counts_not_needed_as_fine() {
        let ok = Readiness { speech: Stage::Ready, ollama: Stage::NotNeeded, cleanup: Stage::Ready, all: false };
        assert!(all_ready(&ok));
        let warming = Readiness { cleanup: Stage::Warming, ..ok.clone() };
        assert!(!all_ready(&warming));
        let failed = Readiness { ollama: Stage::failed("x"), ..ok };
        assert!(!all_ready(&failed));
    }

    #[test]
    fn primary_ollama_is_the_first_cleanup_provider_only() {
        let mut s = Settings::default();
        let p = primary_ollama(&s).expect("Private starts with Ollama");
        assert_eq!(primary_model_at(&s, ollama::DEFAULT_BASE), Some(p.model));
        assert_eq!(primary_model_at(&s, "http://box:11434"), None);
        s.apply_preset(Preset::Fast);
        assert!(primary_ollama(&s).is_none());
    }
}
