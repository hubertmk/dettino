//! Commands invoked by the settings / onboarding UI and the pill.

use crate::app::app;
use crate::controller::CtlMsg;
use crate::pill::{self, LevelFrame, PillState};
use crate::pipeline::{self, Suggestion};
use app_core::stats::StatsSummary;
use app_core::{keys::ChordSpec, Accent, AsrRequest, CleanupLevel, Hotkeys, Preset, ProviderKind, ProviderType, Settings, SoundStyle};
use app_platform::SoundKind;
use app_storage::models::{self, ModelInfo};
use app_storage::HistoryRecord;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::{Channel, Response};
use tauri::{Emitter, Manager};

static MIC: Mutex<Option<Channel<LevelFrame>>> = Mutex::new(None);
pub fn send_mic_level(level: f32, speech: bool) {
    if let Some(ch) = MIC.lock().unwrap().as_ref() {
        let _ = ch.send(LevelFrame { level, speech });
    }
}

#[derive(Serialize)]
pub struct ModelStatus {
    #[serde(flatten)]
    pub info: ModelInfo,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct DeviceView {
    pub name: String,
    pub kind: String,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub keys: HashMap<String, bool>,
    pub ready: HashMap<String, bool>,
    pub stats: HashMap<String, StatsSummary>,
    pub models: Vec<ModelStatus>,
    pub gpus: Vec<app_platform::GpuInfo>,
    pub cpu_threads: usize,
    /// Physical RAM in MB (0 when unknown).
    pub ram_mb: u64,
    pub cleanup_recommendation: crate::ollama::CleanupRecommendation,
    pub ollama_catalog: Vec<crate::ollama::OllamaModel>,
    pub build: crate::build_info::BuildInfo,
    pub data_dir: String,
    pub hotkeys_error: Option<String>,
    pub history_available: bool,
    pub fully_local: bool,
    pub platform: String,
    /// What is loaded and answering right now (see `readiness.rs`).
    pub readiness: crate::readiness::Readiness,
    /// Launch at login: wanted vs. what the OS will do.
    pub autostart: AutostartState,
    /// Where the app runs from (macOS: Applications, translocated, elsewhere).
    pub app_location: app_platform::AppLocation,
}

/// Hardware facts do not change while the app runs; enumerating GPUs on every
/// settings save would.
fn hardware() -> &'static (Vec<app_platform::GpuInfo>, u64) {
    static HW: std::sync::OnceLock<(Vec<app_platform::GpuInfo>, u64)> = std::sync::OnceLock::new();
    HW.get_or_init(|| (app_platform::gpus(), app_platform::total_ram_mb()))
}

pub(crate) fn snapshot() -> Snapshot {
    let a = app();
    let s = a.settings();
    let mut keys = HashMap::new();
    let mut ready = HashMap::new();
    let stats = a.stats.lock().unwrap().clone();
    let mut sums = HashMap::new();
    for p in &s.providers {
        keys.insert(p.id.clone(), a.has_key(&p.id));
        // Ollama counts as ready only once its server answered and listed the model.
        let ok = match p.ptype {
            ProviderType::Ollama => crate::readiness::ollama_usable(p),
            _ => (!p.ptype.needs_key() || a.has_key(&p.id)) && a.local_installed(p),
        };
        ready.insert(p.id.clone(), ok);
        sums.insert(p.id.clone(), stats.summary(&p.id));
    }
    let mdir = a.paths.models();
    let (gpus, ram_mb) = hardware().clone();
    let cleanup_recommendation = crate::ollama::recommend(crate::ollama::Accelerator::detect(&gpus, ram_mb), ram_mb);
    Snapshot {
        fully_local: s.is_fully_local(),
        readiness: crate::readiness::current(),
        autostart: autostart_state(&s),
        app_location: app_platform::app_location(),
        settings: s,
        keys,
        ready,
        stats: sums,
        models: models::catalog().into_iter().map(|m| ModelStatus { installed: models::installed(&mdir, &m), info: m }).collect(),
        gpus,
        ram_mb,
        cleanup_recommendation,
        ollama_catalog: crate::ollama::catalog(),
        cpu_threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        build: crate::build_info::current(),
        data_dir: a.paths.root.to_string_lossy().into(),
        hotkeys_error: a.hotkeys_ok.lock().unwrap().clone().err(),
        history_available: a.history.lock().unwrap().is_some(),
        platform: std::env::consts::OS.into(),
    }
}

#[tauri::command]
pub fn get_state() -> Snapshot {
    snapshot()
}

#[derive(Serialize)]
pub struct AccentInfo {
    palette: Accent,
    /// The OS accent colour (`#RRGGBB`), only for `Accent::System`. The UI clamps it to AA contrast.
    os: Option<String>,
}

/// The accent palette the user picked; the UI maps it to colours from tokens.css.
#[tauri::command]
pub fn accent() -> AccentInfo {
    let palette = app().settings().accent;
    let os = if palette == Accent::System { app_platform::accent_color() } else { None };
    AccentInfo { palette, os }
}

pub fn audio_config(s: &Settings) -> app_audio::AudioConfig {
    app_audio::AudioConfig { device: s.input_device.clone(), mode: s.mic_mode, early_open: s.mic_early_open, preroll_ms: s.preroll_ms, prefer_wired: s.prefer_wired }
}

/// The audio configuration the engine starts with. Until the onboarding is done the
/// microphone opens only for the wizard's own mic test or a dictation, so the system's
/// microphone prompt appears on the step that explains it, not over the welcome page.
/// [`finish_onboarding`] switches to the configured mode.
pub fn startup_audio_config(s: &Settings) -> app_audio::AudioConfig {
    let mut c = audio_config(s);
    if !s.onboarding_done {
        c.mode = app_core::MicMode::OnDemand;
        c.early_open = app_core::MicEarlyOpen::Off;
    }
    c
}

/// Launch at login as the OS sees it, next to what the settings ask for.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AutostartState {
    /// The setting.
    pub wanted: bool,
    /// The OS will start Dettino at the next login.
    pub active: bool,
    /// Why not, when known (switched off in Login Items / Task Manager, not in Applications).
    pub detail: Option<String>,
}

pub(crate) fn autostart_state(s: &Settings) -> AutostartState {
    match app_platform::launch_at_login_active() {
        Ok(active) => AutostartState { wanted: s.launch_at_login, active, detail: None },
        Err(e) => AutostartState { wanted: s.launch_at_login, active: false, detail: Some(e) },
    }
}

pub fn apply_settings(mut new: Settings) -> Result<(), String> {
    let a = app();
    let old = a.settings();
    new.clamp_ranges();
    if new.hotkeys != old.hotkeys {
        let conflicts = new.hotkeys.conflicts();
        if !conflicts.is_empty() {
            return Err(format!("Two actions use the same shortcut: {} and {}", conflicts[0].0, conflicts[0].1));
        }
    }
    // Disk first: a failed save must not leave the running app with settings that
    // the next start will not have.
    app_storage::settings::save(&a.paths.settings(), &new).map_err(|e| e.to_string())?;
    if new.hotkeys != old.hotkeys {
        app_hotkey::set_hotkeys(&new.hotkeys);
    }
    if new.input_device != old.input_device || new.mic_mode != old.mic_mode || new.mic_early_open != old.mic_early_open || new.preroll_ms != old.preroll_ms || new.prefer_wired != old.prefer_wired {
        a.audio.set_config(audio_config(&new));
    }
    if new.paused != old.paused {
        a.audio.pause(new.paused);
    }
    if new.launch_at_login != old.launch_at_login {
        if let Err(e) = app_platform::set_launch_at_login(new.launch_at_login) {
            tracing::warn!("launch at login: {e}");
        }
    }
    let chains_changed = new.asr_chain != old.asr_chain || new.llm_chain != old.llm_chain || new.providers != old.providers;
    let style_changed = new.pill_style != old.pill_style;
    let accent_changed = new.accent != old.accent;
    *a.settings.write().unwrap() = new.clone();
    // Every window re-reads: a change made by the tray, the pill or the backend must
    // not be undone by the next save from a window that still holds the old copy.
    let _ = a.handle.emit("settings:changed", ());
    let _ = a.ctl.send(CtlMsg::Reload);
    crate::tray::refresh();
    if chains_changed || (new.cleanup_level == CleanupLevel::Off) != (old.cleanup_level == CleanupLevel::Off) {
        crate::warmup();
    }
    if style_changed {
        pill::apply_style(new.pill_style);
    }
    if accent_changed {
        let _ = a.handle.emit("accent-changed", ());
    }
    // Re-emits the state with the current settings (sounds, style, success animation, hint).
    pill::set(pill::current());
    Ok(())
}

/// A save from a settings window. Fields the backend owns are taken from the live
/// settings, so a window that saves a copy it took a moment ago cannot revert a
/// pause toggled from the tray, a pill position, or the finished onboarding.
#[tauri::command]
pub fn save_settings(mut settings: Settings) -> Result<Snapshot, String> {
    let live = app().settings();
    settings.paused = live.paused;
    settings.pill_positions = live.pill_positions;
    settings.onboarding_done |= live.onboarding_done;
    apply_settings(settings)?;
    Ok(snapshot())
}

#[tauri::command]
pub fn set_preset(preset: Preset) -> Result<Snapshot, String> {
    let a = app();
    let mut s = a.settings();
    s.apply_preset_with_keys(preset, |id| a.has_key(id));
    apply_settings(s)?;
    Ok(snapshot())
}

#[tauri::command]
pub fn set_provider_key(id: String, key: String) -> Result<Snapshot, String> {
    let a = app();
    let mut s = a.settings();
    if s.provider(&id).is_none() {
        return Err("Unknown provider".into());
    }
    a.set_key(&id, &key)?;
    // One OpenAI key covers both OpenAI transcription and OpenAI cleanup, and
    // removing it removes it from both; a copy left behind would keep uploading.
    if s.provider(&id).is_some_and(is_openai_cloud) {
        let removing = key.trim().is_empty();
        for p in s.providers.iter().filter(|p| p.id != id && is_openai_cloud(p)) {
            if removing || !a.has_key(&p.id) {
                a.set_key(&p.id, &key)?;
            }
        }
    }
    let before = s.llm_chain.clone();
    s.follow_keys(|id| a.has_key(id));
    if s.llm_chain != before {
        apply_settings(s)?; // warms the new chain
    } else {
        crate::warmup();
    }
    Ok(snapshot())
}

/// OpenAI's own API (not a compatible server elsewhere), so keys are interchangeable.
fn is_openai_cloud(p: &app_core::ProviderConfig) -> bool {
    matches!(p.ptype, app_core::ProviderType::OpenaiBatch | app_core::ProviderType::OpenaiChat)
        && p.base_url.as_deref().is_none_or(|b| b.trim().is_empty() || url_host(b) == Some("api.openai.com"))
}

/// The host of a URL, lower-cased: `https://API.openai.com:443/v1` → `api.openai.com`.
fn url_host(url: &str) -> Option<&str> {
    let rest = url.trim().split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

pub fn toggle_pause() {
    let mut s = app().settings();
    s.paused = !s.paused;
    let paused = s.paused;
    let _ = apply_settings(s);
    pill::flash(
        PillState { kind: "notice".into(), label: Some(if paused { "Paused".into() } else { "Dictation is on".into() }), ..Default::default() },
        1200,
    );
}

#[tauri::command]
pub fn set_paused(paused: bool) -> Snapshot {
    let mut s = app().settings();
    if s.paused != paused {
        s.paused = paused;
        let _ = apply_settings(s);
    }
    snapshot()
}

#[derive(Serialize)]
pub struct TestResult {
    pub ok: bool,
    pub ms: u64,
    pub text: Option<String>,
    pub detail: Option<String>,
    pub error: Option<String>,
}

static TEST_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX / 2);

async fn record_sample(ms: u64) -> Result<app_audio::Take, String> {
    let a = app();
    let id = TEST_SESSION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let wav = a.paths.sessions().join(format!("test-{id}.wav"));
    a.audio.begin(id, wav)?;
    tokio::time::sleep(Duration::from_millis(ms)).await;
    let take = a.audio.finish(id).ok_or("Recording failed")?;
    let _ = std::fs::remove_file(&take.wav_path);
    Ok(take)
}

#[tauri::command]
pub async fn test_provider(id: String) -> TestResult {
    let a = app();
    let s = a.settings();
    let Some(cfg) = s.provider(&id).cloned() else {
        return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some("Unknown provider".into()) };
    };
    match cfg.kind() {
        ProviderKind::Asr => {
            let Some(p) = a.asr_provider(&cfg) else {
                return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some("Not available".into()) };
            };
            let take = match record_sample(3000).await {
                Ok(t) => t,
                Err(e) => return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some(e) },
            };
            if !take.has_speech() {
                return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some("Didn't hear anything. Check the microphone in Audio.".into()) };
            }
            let req = AsrRequest { pcm: Arc::new(take.trimmed().to_vec()), language: s.language.clone().unwrap_or_default(), hotwords: vec![], prompt_tail: None };
            let t0 = Instant::now();
            let r = tokio::time::timeout(p.timeout(req.audio_ms()), p.transcribe(&req)).await;
            let ms = t0.elapsed().as_millis() as u64;
            match r {
                Ok(Ok(text)) => {
                    a.record_stat(&id, ms, true, None);
                    TestResult { ok: true, ms, text: Some(text), detail: None, error: None }
                }
                Ok(Err(e)) => {
                    a.record_stat(&id, ms, false, Some(e.short()));
                    TestResult { ok: false, ms, text: None, detail: None, error: Some(e.to_string()) }
                }
                Err(_) => TestResult { ok: false, ms, text: None, detail: None, error: Some("Timed out".into()) },
            }
        }
        ProviderKind::Llm => {
            let Some(p) = a.llm_provider(&cfg) else {
                return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some("Not available".into()) };
            };
            let sample = "um so let's meet at two actually three on thursday";
            let req = app_cleanup::build_cleanup_request(app_core::CleanupLevel::Light, "", &Default::default(), &[], sample);
            let t0 = Instant::now();
            let r = tokio::time::timeout(Duration::from_secs(30), p.complete(&req)).await;
            let ms = t0.elapsed().as_millis() as u64;
            match r {
                Ok(Ok(resp)) => {
                    a.record_stat(&id, ms, true, None);
                    let text = app_cleanup::parse_cleanup(&resp.text).map(|p| p.text).unwrap_or(resp.text);
                    TestResult { ok: true, ms, text: Some(text), detail: Some(format!("{sample} → model {}", resp.model)), error: None }
                }
                Ok(Err(e)) => {
                    a.record_stat(&id, ms, false, Some(e.short()));
                    TestResult { ok: false, ms, text: None, detail: None, error: Some(e.to_string()) }
                }
                Err(_) => TestResult { ok: false, ms, text: None, detail: None, error: Some("Timed out after 30 s".into()) },
            }
        }
    }
}

/// Run a short recording through the active transcription chain (mic check).
#[tauri::command]
pub async fn mic_check() -> TestResult {
    let a = app();
    let s = a.settings();
    let take = match record_sample(3500).await {
        Ok(t) => t,
        Err(e) => return TestResult { ok: false, ms: 0, text: None, detail: None, error: Some(e) },
    };
    let detail = Some(format!("Peak {:.0} dBFS", take.max_db));
    if !take.has_speech() {
        return TestResult { ok: false, ms: 0, text: None, detail, error: Some(format!("Nothing picked up on {}. Try another microphone.", take.device)) };
    }
    for cfg in a.chain(&s.asr_chain) {
        let Some(p) = a.asr_provider(&cfg) else { continue };
        let req = AsrRequest { pcm: Arc::new(take.trimmed().to_vec()), language: s.language.clone().unwrap_or_default(), hotwords: vec![], prompt_tail: None };
        let t0 = Instant::now();
        if let Ok(Ok(text)) = tokio::time::timeout(p.timeout(req.audio_ms()), p.transcribe(&req)).await {
            return TestResult { ok: true, ms: t0.elapsed().as_millis() as u64, text: Some(text), detail, error: None };
        }
    }
    TestResult { ok: false, ms: 0, text: None, detail, error: Some("No transcription provider answered. Set one up first.".into()) }
}

#[tauri::command]
pub fn list_devices() -> Vec<DeviceView> {
    app_audio::list_devices()
        .into_iter()
        .map(|d| DeviceView { name: d.name, kind: format!("{:?}", d.kind).to_lowercase(), is_default: d.is_default })
        .collect()
}

#[tauri::command]
pub fn mic_monitor(on: bool, channel: Channel<LevelFrame>) {
    *MIC.lock().unwrap() = if on { Some(channel) } else { None };
    app().audio.monitor(on);
}

#[tauri::command]
pub fn subscribe_levels(channel: Channel<LevelFrame>) {
    pill::subscribe_levels(channel);
}

#[tauri::command]
pub fn pill_ready() -> PillState {
    pill::current()
}

#[tauri::command]
pub fn pill_rect(x: f64, y: f64, w: f64, h: f64) {
    pill::set_rect(x, y, w, h);
}

#[tauri::command]
pub fn pill_action(id: String) {
    let _ = app().ctl.send(CtlMsg::PillAction(id));
}

#[tauri::command]
pub fn pill_reset_position() {
    pill::reset_position();
}

/// Settings → Feedback: the start sound, then the stop sound, in `style` at the saved volume.
#[tauri::command]
pub fn preview_sound(style: SoundStyle) {
    let volume = app().settings().sound_volume;
    std::thread::spawn(move || {
        app_platform::play_sound(SoundKind::Start, style, volume);
        std::thread::sleep(Duration::from_millis(700));
        app_platform::play_sound(SoundKind::Stop, style, volume);
    });
}

/// When the shortcut recorder suspended the chords; a safety net restores them.
static SUSPENDED_AT: Mutex<Option<Instant>> = Mutex::new(None);
const SUSPEND_MAX: Duration = Duration::from_secs(60);

/// While the shortcut recorder is open, the hook must not swallow chords.
#[tauri::command]
pub fn hotkeys_suspend(suspend: bool) {
    if suspend {
        let none = ChordSpec::new(0, 0);
        app_hotkey::set_hotkeys(&Hotkeys { push_to_talk: none, hands_free: none, command: none, undo_ai: none, paste_last: none });
        *SUSPENDED_AT.lock().unwrap() = Some(Instant::now());
    } else {
        *SUSPENDED_AT.lock().unwrap() = None;
        app_hotkey::set_hotkeys(&app().settings().hotkeys);
    }
}

/// Called from the controller tick: a recorder that went away without resuming
/// (window hidden, reload, crash) must not leave every shortcut dead.
pub fn resume_hotkeys_if_stale() {
    let stale = SUSPENDED_AT.lock().unwrap().is_some_and(|t| t.elapsed() > SUSPEND_MAX);
    if stale {
        tracing::warn!("shortcuts were left suspended; restoring them");
        hotkeys_suspend(false);
    }
}

/// Model ids being downloaded: a second download of the same id would write the same `.part` file.
static DOWNLOADING: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[tauri::command]
pub async fn model_download(id: String) -> Result<Snapshot, String> {
    let a = app();
    let m = models::catalog().into_iter().find(|m| m.id == id).ok_or("Unknown model")?;
    {
        let mut busy = DOWNLOADING.lock().unwrap();
        if busy.contains(&id) {
            return Err(format!("{} is already downloading.", m.name));
        }
        busy.push(id.clone());
    }
    let r = download_model(a, &m).await;
    DOWNLOADING.lock().unwrap().retain(|x| x != &id);
    r
}

async fn download_model(a: &crate::app::App, m: &ModelInfo) -> Result<Snapshot, String> {
    let handle = a.handle.clone();
    pill::set_badge(Some("downloading"), Some(&format!("Downloading {}", m.name)));
    let r = models::download(&a.paths.models(), m, move |p| {
        let _ = handle.emit("model:progress", p);
    })
    .await;
    pill::clear_badge("downloading");
    r?;
    if m.family == "vad" {
        crate::install_vad();
    }
    crate::warmup();
    Ok(snapshot())
}

/// Free disk space in MB where the setup downloads go: the lower of Dettino's model
/// folder and Ollama's (`OLLAMA_MODELS`, else `~/.ollama/models`). `None` when unknown.
#[tauri::command]
pub fn free_disk_mb() -> Option<u64> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from);
    let ollama = std::env::var_os("OLLAMA_MODELS").map(std::path::PathBuf::from).or_else(|| home.map(|h| h.join(".ollama").join("models")));
    [Some(app().paths.models()), ollama].into_iter().flatten().filter_map(|p| app_platform::free_disk_mb(&p)).min()
}

#[tauri::command]
pub async fn model_delete(id: String) -> Result<Snapshot, String> {
    let a = app();
    let m = models::catalog().into_iter().find(|m| m.id == id).ok_or("Unknown model")?;
    tokio::task::spawn_blocking(move || models::remove(&a.paths.models(), &m).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())??;
    Ok(snapshot())
}

#[tauri::command]
pub fn history_list(query: Option<String>, limit: Option<usize>) -> Vec<HistoryRecord> {
    app().with_history(|h| h.list(limit.unwrap_or(200), query.as_deref())).unwrap_or_default()
}

#[tauri::command]
pub fn history_delete(id: String) -> Result<(), String> {
    app().with_history(|h| h.delete(&id)).unwrap_or(Ok(()))
}

#[tauri::command]
pub async fn history_delete_all() -> Result<(), String> {
    tokio::task::spawn_blocking(history_delete_all_blocking).await.map_err(|e| e.to_string())?
}

fn history_delete_all_blocking() -> Result<(), String> {
    let a = app();
    let audio = a.paths.audio();
    // Crash-recovery recordings in sessions/ are recordings too.
    if let Ok(rd) = std::fs::read_dir(a.paths.sessions()) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    a.with_history(|h| h.delete_all(&audio)).unwrap_or(Ok(()))
}

/// Recordings longer than this are not sent to the webview in one piece.
const MAX_AUDIO_BYTES: u64 = 64 * 1024 * 1024;

#[tauri::command]
pub async fn history_audio(id: String) -> Result<Response, String> {
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let path = app()
            .with_history(|h| h.get(&id).map(|r| h.audio_file(&r)))
            .flatten()
            .ok_or("Not found")?
            .ok_or("Audio was deleted after the retention period")?;
        let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if len > MAX_AUDIO_BYTES {
            return Err("This recording is too long to play here; open the data folder instead.".into());
        }
        std::fs::read(path).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Response::new(bytes))
}

#[tauri::command]
pub fn history_retry(id: String) -> Result<(), String> {
    let job = pipeline::job_from_history(&id, false).ok_or("The recording for this item is no longer available")?;
    app().jobs.send(job).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> bool {
    app_platform::set_clipboard_text(&text)
}

#[tauri::command]
pub fn suggestions_list() -> Vec<Suggestion> {
    app().with_history(|h| h.kv_get::<Vec<Suggestion>>("suggestions")).flatten().unwrap_or_default()
}

#[tauri::command]
pub fn suggestion_resolve(to: String, accept: bool) -> Result<Snapshot, String> {
    let a = app();
    let mut list: Vec<Suggestion> = a.with_history(|h| h.kv_get("suggestions")).flatten().unwrap_or_default();
    if let Some(pos) = list.iter().position(|s| s.to == to) {
        let s = list.remove(pos);
        if accept {
            let mut settings = a.settings();
            if !settings.dictionary.iter().any(|d| d.term.eq_ignore_ascii_case(&s.to)) {
                settings.dictionary.push(app_core::DictionaryEntry {
                    term: s.to,
                    replace_from: Some(s.from),
                    source: "auto".into(),
                    enabled: true,
                });
                apply_settings(settings)?;
            }
        }
        a.with_history(|h| h.kv_set("suggestions", &list));
    }
    Ok(snapshot())
}

#[derive(Serialize)]
pub struct ContextPreview {
    pub app: String,
    pub title: String,
    pub context: app_platform::ContextSnapshot,
    pub nouns: Vec<String>,
    pub providers: Vec<(String, bool)>,
    pub rule_context: bool,
}

/// "What would be sent": capture the app focused 3 s from now.
#[tauri::command]
pub async fn preview_context() -> ContextPreview {
    tokio::time::sleep(Duration::from_secs(3)).await;
    let a = app();
    let s = a.settings();
    let fg = app_platform::foreground().unwrap_or_default();
    let rule_ctx = s.rule_for(&fg.app).and_then(|r| r.context).unwrap_or(true);
    let on = s.context.enabled && rule_ctx;
    let req = app_platform::ContextRequest {
        caret_text: on && s.context.caret_text,
        selection: on && s.context.selection,
        visible_text: on && s.context.visible_text,
    };
    let context = tokio::task::spawn_blocking(move || app_platform::capture_context(req, Duration::from_millis(400))).await.unwrap_or_default();
    let nouns = context.visible_text.as_deref().map(|t| app_cleanup::nouns::extract(t, 50)).unwrap_or_default();
    let providers = s
        .asr_chain
        .iter()
        .chain(s.llm_chain.iter())
        .filter_map(|id| s.provider(id))
        .map(|p| (p.display_name.clone(), on && (p.is_local() || !s.context.local_only)))
        .collect();
    ContextPreview {
        app: if on && s.context.app_name { fg.app } else { String::new() },
        title: if on && s.context.window_title { fg.title } else { String::new() },
        context,
        nouns,
        providers,
        rule_context: rule_ctx,
    }
}

#[tauri::command]
pub fn finish_onboarding() -> Result<Snapshot, String> {
    let a = app();
    let mut s = a.settings();
    s.onboarding_done = true;
    apply_settings(s.clone())?;
    // The microphone may now stay open as configured (see `startup_audio_config`).
    a.audio.set_config(audio_config(&s));
    // Launch at login was only a setting until now; the last page showed the switch.
    if s.launch_at_login {
        if let Err(e) = app_platform::set_launch_at_login(true) {
            tracing::warn!("launch at login: {e}");
        }
    }
    // The wizard is done: the window goes away and the pill takes over. It says so
    // only when everything is really running (not after "Skip setup").
    if let Some(w) = a.handle.get_webview_window("main") {
        let _ = w.hide();
        after_hide();
    }
    if crate::readiness::current().all {
        let chord = a.settings().hotkeys.push_to_talk.label();
        pill::flash(PillState { kind: "ready".into(), label: Some(format!("Pronto! Hold {chord} to dictate")), ..Default::default() }, 4000);
    }
    Ok(snapshot())
}

/// Load the models again (the setup's "Try again" after a failed warm-up).
#[tauri::command]
pub fn warm_now() {
    crate::warmup();
}

#[tauri::command]
pub fn open_data_folder(which: String) -> Result<(), String> {
    let a = app();
    let p = match which.as_str() {
        "logs" => a.paths.logs(),
        "models" => a.paths.models(),
        // The onboarding's "move Dettino to Applications" nudge on macOS.
        "applications" => std::path::PathBuf::from("/Applications"),
        _ => a.paths.root.clone(),
    };
    use tauri_plugin_opener::OpenerExt;
    a.handle.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// The main window was closed: dictation keeps running from the menu bar / tray.
pub fn hide_main(w: &tauri::Window) {
    let _ = w.hide();
    after_hide();
}

/// What a hidden main window must not keep: suspended shortcuts, an open
/// microphone, a Dock icon.
fn after_hide() {
    // A shortcut recorder that was open in the window must not leave the chords off.
    if SUSPENDED_AT.lock().unwrap().is_some() {
        hotkeys_suspend(false);
    }
    // The Audio page's level meter keeps the microphone open; a hidden window is not
    // "only while dictating".
    *MIC.lock().unwrap() = None;
    app().audio.monitor(false);
    #[cfg(target_os = "macos")]
    let _ = app().handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// App rules on macOS match bundle identifiers: look one up from an app's name.
#[tauri::command]
pub fn resolve_app(name: String) -> Option<String> {
    #[cfg(target_os = "macos")]
    return app_platform::bundle_id_for(&name);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        None
    }
}

#[tauri::command]
pub fn permissions() -> app_platform::Permissions {
    app_platform::permissions()
}

/// Show the system prompt for a permission (macOS asks only once per permission).
#[tauri::command]
pub fn request_permission(kind: String) -> app_platform::Permissions {
    app_platform::request_permission(&kind);
    app_platform::permissions()
}

/// Open the System Settings page for a permission.
#[tauri::command]
pub fn open_permission_settings(kind: String) -> Result<(), String> {
    let url = match kind.as_str() {
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "input_monitoring" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
        "microphone" if cfg!(target_os = "macos") => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        "microphone" => "ms-settings:privacy-microphone",
        _ => return Err(format!("unknown permission {kind}")),
    };
    use tauri_plugin_opener::OpenerExt;
    app().handle.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_settings() -> String {
    serde_json::to_string_pretty(&app().settings()).unwrap_or_default()
}

#[tauri::command]
pub async fn import_settings(json: String) -> Result<Snapshot, String> {
    let s: Settings = serde_json::from_str(&json).map_err(|e| format!("That file is not a settings export: {e}"))?;
    tokio::task::spawn_blocking(move || apply_settings(s)).await.map_err(|e| e.to_string())??;
    Ok(snapshot())
}

#[tauri::command]
pub fn show_main(section: Option<String>) {
    let a = app();
    if let Some(w) = a.handle.get_webview_window("main") {
        // A Dock icon and ⌘Tab entry only while the window is open.
        #[cfg(target_os = "macos")]
        let _ = a.handle.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        if let Some(s) = section {
            let _ = a.handle.emit_to("main", "navigate", s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{startup_audio_config, url_host};
    use app_core::{MicEarlyOpen, MicMode, Settings};

    #[test]
    fn the_microphone_stays_closed_until_the_onboarding_asks_for_it() {
        // A warm microphone would open (and prompt) the moment the app starts.
        let s = Settings { mic_mode: MicMode::AlwaysWarm, mic_early_open: MicEarlyOpen::Hold, ..Settings::default() };
        let c = startup_audio_config(&s);
        assert_eq!((c.mode, c.early_open), (MicMode::OnDemand, MicEarlyOpen::Off), "before onboarding: only a mic test or a dictation opens it");
        let done = Settings { onboarding_done: true, ..s };
        let c = startup_audio_config(&done);
        assert_eq!((c.mode, c.early_open), (MicMode::AlwaysWarm, MicEarlyOpen::Hold), "after onboarding: as configured");
    }

    #[test]
    fn hosts_are_compared_exactly() {
        assert_eq!(url_host("https://api.openai.com/v1"), Some("api.openai.com"));
        assert_eq!(url_host("https://API.openai.com:443/v1"), Some("API.openai.com"));
        assert_eq!(url_host("https://api.openai.com.evil.tld/v1"), Some("api.openai.com.evil.tld"));
        assert_eq!(url_host("http://user@localhost:1234/v1"), Some("localhost"));
        assert_eq!(url_host("not a url"), None);
    }
}
