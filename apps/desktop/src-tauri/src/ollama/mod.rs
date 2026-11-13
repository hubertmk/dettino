//! In-app Ollama setup: detect it, install or start it, pull a cleanup model with
//! progress, and recommend a model for this computer. Everything OS-specific
//! (installer, paths, process spawning) lives in `win.rs` / `mac.rs` / `unsupported.rs`;
//! the catalog, the recommendation rule, the commands and the events are shared.

use crate::app::app;
use crate::commands::{self, Snapshot};
use crate::pill;
use app_llm::ollama::{PullStage, PullTracker};
use app_platform::GpuInfo;
use app_storage::models::{download_file, Progress};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as os;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as os;
#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(not(any(windows, target_os = "macos")))]
use unsupported as os;

pub const DEFAULT_BASE: &str = "http://localhost:11434";
/// Progress id of the Ollama installer download (`model:progress` event).
pub const SETUP_ID: &str = "ollama:setup";
/// File in the data folder that says Dettino installed Ollama. The Windows uninstaller
/// reads it to tick "Also remove Ollama" by default (installer.nsi, un.ConfirmShow).
pub const INSTALLED_MARKER: &str = "ollama-installed-by-dettino";

/// Ollama is installed on this computer (it may not be running).
pub fn installed() -> bool {
    os::installed()
}

/// Remove Ollama and its models (the uninstaller's "Also remove Ollama" box).
#[cfg(windows)]
pub fn remove() -> Result<(), String> {
    win::uninstall()
}

// ---------------------------------------------------------------- catalog

/// A cleanup model the onboarding offers. Download sizes from ollama.com; memory from
/// Ollama's resident size at `num_ctx 4096` plus headroom, and speeds from measurements
/// on real dictations (October 2026).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OllamaModel {
    pub tag: String,
    pub name: String,
    /// Approximate download size.
    pub download_bytes: u64,
    /// RAM or GPU memory the model needs while loaded.
    pub memory_mb: u64,
    pub cpu_note: String,
    pub gpu_note: String,
}

pub fn catalog() -> Vec<OllamaModel> {
    // One model only: in measurements the larger Gemma 4 E4B was not better on real
    // dictations (judge 4.37 vs 4.63) and is far slower without a GPU.
    vec![OllamaModel {
        tag: MODEL.into(),
        name: "Gemma 4 E2B".into(),
        download_bytes: 4_300_000_000,
        memory_mb: 2_500,
        cpu_note: "Several seconds per cleanup on the processor".into(),
        gpu_note: "About half a second on an NVIDIA GPU, faster on large cards".into(),
    }]
}

// ---------------------------------------------------------------- recommendation

/// What can run a local model fast on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator {
    Nvidia { vram_mb: u64 },
    AppleSilicon { unified_mb: u64 },
    None,
}

impl Accelerator {
    pub fn detect(gpus: &[GpuInfo], ram_mb: u64) -> Self {
        if let Some(v) = gpus.iter().filter(|g| g.vendor == "NVIDIA").map(|g| g.vram_mb).max() {
            return Self::Nvidia { vram_mb: v };
        }
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Self::AppleSilicon { unified_mb: ram_mb };
        }
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CleanupRecommendation {
    pub model: String,
    pub reason: String,
    /// Cleanup will take several seconds; the UI shows a warning.
    pub slow_cpu: bool,
    pub tier: &'static str,
}

/// The cleanup model the onboarding installs.
pub const MODEL: &str = "gemma4:e2b-it-qat";
const LOW_RAM_MB: u64 = 16_384;

fn gb(mb: u64) -> u64 {
    (mb + 512) / 1024
}

/// Which speed to expect from [`MODEL`] here, and whether to warn that cleanup is slow.
pub fn recommend(acc: Accelerator, ram_mb: u64) -> CleanupRecommendation {
    let rec = |reason: String, slow_cpu: bool, tier: &'static str| CleanupRecommendation { model: MODEL.into(), reason, slow_cpu, tier };
    match acc {
        Accelerator::Nvidia { vram_mb } => rec(
            format!("Your NVIDIA GPU has {} GB of memory: the cleanup model answers in about half a second.", gb(vram_mb)),
            false,
            "gpu",
        ),
        Accelerator::AppleSilicon { unified_mb } => rec(
            format!("Apple Silicon with {} GB of memory: the cleanup model fits comfortably.", gb(unified_mb)),
            false,
            "apple",
        ),
        Accelerator::None if ram_mb > 0 && ram_mb < LOW_RAM_MB => rec(
            format!("No NVIDIA GPU and {} GB of RAM: the cleanup model fits, but cleanup may take several seconds. Dettino waits for it.", gb(ram_mb)),
            true,
            "cpu_low_ram",
        ),
        Accelerator::None => rec("No NVIDIA GPU: the cleanup model runs on the processor in a few seconds.".into(), false, "cpu"),
    }
}

// ---------------------------------------------------------------- status / lifecycle

#[derive(Debug, Clone, Serialize, Default)]
pub struct OllamaStatus {
    /// Ollama is installed on this computer (may not be running).
    pub installed: bool,
    /// The API answers at `base`.
    pub reachable: bool,
    pub models: Vec<String>,
    pub version: Option<String>,
    /// The app can install Ollama itself on this OS.
    pub can_install: bool,
}

pub async fn status(base: &str) -> OllamaStatus {
    let models = app_llm::Ollama::list_models(base).await.ok();
    let version = if models.is_some() { app_llm::ollama::version(base).await.ok() } else { None };
    OllamaStatus { installed: os::installed() || models.is_some(), reachable: models.is_some(), models: models.unwrap_or_default(), version, can_install: os::SUPPORTED }
}

pub async fn reachable(base: &str) -> bool {
    app_llm::Ollama::list_models(base).await.is_ok()
}

pub async fn wait_reachable(base: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if reachable(base).await {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// The server runs on this computer, so Dettino may start it. A remote Ollama never is.
pub fn is_local_base(base: &str) -> bool {
    let rest = base.split_once("://").map_or(base, |(_, r)| r);
    let host = rest.split('/').next().unwrap_or("");
    let host = if host.starts_with('[') { host.split(']').next().map(|h| &h[1..]).unwrap_or("") } else { host.split(':').next().unwrap_or("") };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

static LAUNCHING: AtomicBool = AtomicBool::new(false);

struct LaunchGuard;
impl Drop for LaunchGuard {
    fn drop(&mut self) {
        LAUNCHING.store(false, Ordering::SeqCst);
    }
}

/// Make sure an installed Ollama is running and wait up to `budget` for its API.
/// `grace` is how long to give an Ollama that may already be starting (its own login
/// item) before starting it ourselves. Returns whether Dettino started it. A second
/// caller while a start is under way only waits for the API instead of starting it again.
pub async fn ensure_running(base: &str, grace: Duration, budget: Duration) -> Result<bool, String> {
    if reachable(base).await {
        return Ok(false);
    }
    if LAUNCHING.swap(true, Ordering::SeqCst) {
        return if wait_reachable(base, budget).await { Ok(false) } else { Err(not_answering(budget)) };
    }
    let _guard = LaunchGuard;
    if !os::installed() {
        return Err("Ollama is not installed.".into());
    }
    if !grace.is_zero() && wait_reachable(base, grace).await {
        return Ok(false);
    }
    os::launch()?;
    tracing::info!("started Ollama");
    if wait_reachable(base, budget).await {
        Ok(true)
    } else {
        Err(not_answering(budget))
    }
}

fn not_answering(budget: Duration) -> String {
    format!("Ollama was started but its API did not answer within {} seconds.", budget.as_secs())
}

/// Start an installed Ollama and wait for its API.
pub async fn launch(base: &str) -> Result<(), String> {
    ensure_running(base, Duration::ZERO, Duration::from_secs(20)).await.map(|_| ())
}

static INSTALLING: AtomicBool = AtomicBool::new(false);
/// Woken by [`ollama_cancel`]; only waiters at that moment are affected.
static CANCEL: tokio::sync::Notify = tokio::sync::Notify::const_new();
/// Longest an installer run may take before it is killed and reported.
const INSTALLER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

struct InstallGuard;
impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// Download the installer, verify its signature, run it silently, wait for the API.
/// Progress goes out on `model:progress` with id `ollama:setup`.
pub async fn install(base: &str) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("Ollama is already being installed.".into());
    }
    let _guard = InstallGuard;
    tokio::select! {
        r = install_inner(base) => r,
        _ = CANCEL.notified() => Err("The Ollama installation was cancelled.".into()),
    }
}

async fn install_inner(base: &str) -> Result<(), String> {
    if !os::SUPPORTED {
        return Err("Installing Ollama from inside Dettino is not available on this system yet. Use the download page.".into());
    }
    let a = app();
    let handle = a.handle.clone();
    let emit = move |p: Progress| {
        let _ = handle.emit("model:progress", p);
    };
    let cache = a.paths.root.join("cache");
    let part = cache.join(format!("{}.part", os::SETUP_FILE));
    let exe = cache.join(os::SETUP_FILE);
    // Always a fresh download: the URL points at the latest release, so resuming a
    // partial file from an earlier day could splice two releases together.
    let _ = tokio::fs::remove_file(&part).await;
    let _ = tokio::fs::remove_file(&exe).await;
    pill::set_badge(Some("downloading"), Some("Downloading Ollama"));
    let r = async {
        let done = download_file(os::SETUP_URL, &part, 0, SETUP_ID, &emit).await?;
        tokio::fs::rename(&part, &exe).await.map_err(|e| e.to_string())?;
        emit(Progress::new(SETUP_ID, done, done, "installing"));
        // The installer is checked against its publisher's signature before it runs.
        tokio::time::timeout(INSTALLER_TIMEOUT, os::run_installer(&exe))
            .await
            .map_err(|_| "The Ollama installer did not finish within 20 minutes.".to_string())??;
        let _ = std::fs::write(a.paths.root.join(INSTALLED_MARKER), b"");
        emit(Progress::new(SETUP_ID, done, done, "starting"));
        crate::readiness::set_ollama(crate::readiness::Stage::Warming);
        // The installer may start Ollama itself; give it a moment, then start it
        // ourselves, through the same guard the launch sequence uses.
        if !wait_reachable(base, Duration::from_secs(5)).await {
            ensure_running(base, Duration::ZERO, Duration::from_secs(30))
                .await
                .map_err(|e| format!("Ollama was installed but its API did not answer. {} ({e})", os::START_HINT))?;
        }
        emit(Progress::new(SETUP_ID, done, done, "ready"));
        Ok::<(), String>(())
    }
    .await;
    pill::clear_badge("downloading");
    let _ = tokio::fs::remove_file(&exe).await;
    let _ = tokio::fs::remove_file(&part).await;
    r
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn ollama_status(base: Option<String>) -> OllamaStatus {
    let base = base.unwrap_or_else(|| DEFAULT_BASE.into());
    let st = status(&base).await;
    observe(&base, &st);
    st
}

/// Tell readiness about a probe of `base`.
fn observe(base: &str, st: &OllamaStatus) {
    let model = crate::readiness::primary_model_at(&app().settings(), base);
    crate::readiness::observe_ollama(base, st, model.as_deref(), true);
}

/// Ollama is up: record it, warm the model, report.
async fn started(base: &str) -> Result<OllamaStatus, String> {
    let st = status(base).await;
    observe(base, &st);
    crate::warmup();
    Ok(st)
}

/// Stop a running installation (download or installer); the model pull is not affected.
#[tauri::command]
pub fn ollama_cancel() {
    CANCEL.notify_waiters();
}

#[tauri::command]
pub async fn ollama_install(base: Option<String>) -> Result<OllamaStatus, String> {
    let base = base.unwrap_or_else(|| DEFAULT_BASE.into());
    let r = install(&base).await.map(|()| true);
    crate::readiness::set_ollama(crate::readiness::stage_after_start(&r, installed()));
    r?;
    started(&base).await
}

#[tauri::command]
pub async fn ollama_launch(base: Option<String>) -> Result<OllamaStatus, String> {
    let base = base.unwrap_or_else(|| DEFAULT_BASE.into());
    crate::readiness::set_ollama(crate::readiness::Stage::Warming);
    let r = launch(&base).await.map(|()| true);
    crate::readiness::set_ollama(crate::readiness::stage_after_start(&r, installed()));
    r?;
    started(&base).await
}

/// Pull a model and make it the Ollama cleanup model.
#[tauri::command]
pub async fn ollama_pull(model: String, base: Option<String>) -> Result<Snapshot, String> {
    let base = base.unwrap_or_else(|| DEFAULT_BASE.into());
    let a = app();
    let handle = a.handle.clone();
    let id = format!("ollama:{model}");
    let name = catalog().into_iter().find(|m| m.tag == model).map(|m| m.name).unwrap_or_else(|| model.clone());
    pill::set_badge(Some("downloading"), Some(&format!("Downloading {name}")));
    let mut tracker = PullTracker::default();
    let r = app_llm::ollama::pull(&base, &model, |ev| {
        if tracker.feed(ev) {
            let mut p = Progress::new(&id, tracker.done, tracker.total, tracker.stage.as_str());
            p.detail = Some(tracker.status.clone());
            let _ = handle.emit("model:progress", p);
        }
    })
    .await;
    pill::clear_badge("downloading");
    r.map_err(|e| e.to_string())?;
    if tracker.stage != PullStage::Ready {
        let mut p = Progress::new(&id, tracker.total, tracker.total, "ready");
        p.detail = Some("success".into());
        let _ = a.handle.emit("model:progress", p);
    }
    let mut s = a.settings();
    if let Some(p) = s.providers.iter_mut().find(|p| p.id == "ollama") {
        p.model = model;
    }
    commands::apply_settings(s)?;
    crate::readiness::set_cleanup(crate::readiness::Stage::Warming);
    crate::warmup();
    Ok(commands::snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv(vram_mb: u64) -> Vec<GpuInfo> {
        vec![GpuInfo { name: "NVIDIA GeForce".into(), vram_mb, vendor: "NVIDIA".into() }]
    }

    #[test]
    fn catalog_is_the_default_provider_model() {
        let c = catalog();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].tag, MODEL);
        let default_model = app_core::default_providers().into_iter().find(|p| p.id == "ollama").unwrap().model;
        assert_eq!(default_model, MODEL, "the shipped default must be the model the onboarding installs");
    }

    #[test]
    fn only_this_computer_is_local() {
        for b in ["http://localhost:11434", "http://127.0.0.1:11434/", "http://[::1]:11434", "localhost:11434", "HTTP://LocalHost"] {
            assert!(is_local_base(b), "{b}");
        }
        for b in ["http://192.0.2.20:11434", "https://ollama.example.com", "http://localhost.example.com:11434", ""] {
            assert!(!is_local_base(b), "{b}");
        }
    }

    #[test]
    fn recommendation_follows_hardware() {
        let r = |gpus: &[GpuInfo], ram| recommend(Accelerator::detect(gpus, ram), ram);
        for vram in [24_463, 12_281, 8_191, 6_144] {
            let x = r(&nv(vram), 16_384);
            assert_eq!(x.model, MODEL);
            assert!(!x.slow_cpu, "{vram} MB card");
            assert_eq!(x.tier, "gpu");
        }
        // Without an NVIDIA card, detection depends on the machine the test runs on.
        let apple = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(matches!(Accelerator::detect(&[], 32_768), Accelerator::AppleSilicon { .. }), apple);
        let cpu = |ram| recommend(Accelerator::None, ram);
        assert!(!cpu(32_768).slow_cpu);
        assert!(cpu(8_192).slow_cpu);
        assert!(!cpu(0).slow_cpu, "unknown RAM must not warn");
        if !apple {
            let amd = vec![GpuInfo { name: "AMD Radeon".into(), vram_mb: 16_384, vendor: "AMD".into() }];
            assert_eq!(r(&amd, 32_768).tier, "cpu");
        }
        let mac = recommend(Accelerator::AppleSilicon { unified_mb: 8_192 }, 8_192);
        assert_eq!((mac.model.as_str(), mac.slow_cpu, mac.tier), (MODEL, false, "apple"));
    }
}
