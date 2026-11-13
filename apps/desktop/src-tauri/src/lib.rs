//! Desktop app: wiring of hotkeys, audio, providers, pipeline and UI.

/// The product name shown to people. Identifiers (bundle id, data folder, registry value) stay unchanged.
pub const APP_NAME: &str = "Dettino";

/// The paste shortcut as people press it.
pub const PASTE_KEYS: &str = if cfg!(target_os = "macos") { "⌘V" } else { "Ctrl+V" };

mod app;
mod build_info;
mod commands;
mod controller;
mod expiry;
mod ollama;
mod pill;
mod pipeline;
mod readiness;
mod startup;
mod tray;
mod uninstall;

use app::{app, now_ms, App};
use app_audio::{AudioEngine, EnergyVad, Vad};
use app_core::ProviderType;
use app_storage::{history, secrets, settings as settings_store, History, HistoryRecord, Paths, Status};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent};

fn init_logging(paths: &Paths) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("dettino")
        .filename_suffix("log")
        .max_log_files(7)
        .build(paths.logs())
        .ok();
    let filter = EnvFilter::try_from_env("DETTINO_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter).with(fmt::layer().with_target(false));
    if let Some(f) = file {
        let (nb, guard) = tracing_appender::non_blocking(f);
        Box::leak(Box::new(guard));
        let _ = registry.with(fmt::layer().json().with_writer(nb)).try_init();
    } else {
        let _ = registry.try_init();
    }
}

/// Use Silero if its model is present, else the energy fallback.
pub fn install_vad() {
    let a = app();
    let model = a.paths.models().join("vad").join("silero_vad.onnx");
    if model.exists() {
        a.audio.set_vad_factory(Box::new(move || match app_asr_local::silero::SileroVad::new(&model) {
            Ok(v) => Box::new(v) as Box<dyn Vad>,
            Err(e) => {
                tracing::warn!("Silero VAD unavailable ({e}); using energy VAD");
                Box::new(EnergyVad::default())
            }
        }));
        tracing::info!("Silero VAD enabled");
    }
}

/// One warm pass at a time. A request during a pass runs one more pass afterwards,
/// so the final state (after a download, an install and a pull in a row) is what
/// gets warmed, not three overlapping copies of it.
struct Flight {
    running: bool,
    again: bool,
}

impl Flight {
    fn request(&mut self) -> bool {
        if self.running {
            self.again = true;
            false
        } else {
            self.running = true;
            true
        }
    }

    fn finish(&mut self) -> bool {
        if self.again {
            self.again = false;
            true
        } else {
            self.running = false;
            false
        }
    }
}

static FLIGHT: Mutex<Flight> = Mutex::new(Flight { running: false, again: false });

/// Take the warm flight; `false` when a pass is running (it will run once more).
pub(crate) fn warm_begin() -> bool {
    FLIGHT.lock().unwrap().request()
}

/// Release the flight; `true` when another pass was asked for meanwhile.
pub(crate) fn warm_finish() -> bool {
    FLIGHT.lock().unwrap().finish()
}

/// Load the first local models and open connections so the first dictation is fast.
pub fn warmup() {
    if warm_begin() {
        warm_continue();
    }
}

/// Run warm passes until nobody asks for another one.
pub(crate) fn warm_continue() {
    tauri::async_runtime::spawn(async {
        loop {
            warm_once().await;
            if !warm_finish() {
                break;
            }
        }
    });
}

async fn warm_once() {
    readiness::begin(&app().settings());
    auto_pick_ollama_model().await;
    warm_asr().await;
    let a = app();
    let s = a.settings();
    if let Some(cfg) = a.chain(&s.llm_chain).first().cloned() {
        warm_llm(&cfg).await;
    }
}

/// Load the first transcription model. Only the configured primary reports to
/// readiness; a fallback that happens to be installed does not stand in for it.
async fn warm_asr() {
    let a = app();
    let s = a.settings();
    let Some(cfg) = a.chain(&s.asr_chain).first().cloned() else { return };
    let primary = s.asr_chain.first() == Some(&cfg.id);
    let Some(p) = a.asr_provider(&cfg) else { return };
    let t0 = std::time::Instant::now();
    match p.warmup().await {
        Ok(()) => {
            tracing::info!(provider = %cfg.id, ms = t0.elapsed().as_millis() as u64, "transcription warm");
            if primary {
                readiness::set_speech(readiness::Stage::Ready);
            }
        }
        Err(e) => {
            tracing::warn!(provider = %cfg.id, "warmup failed: {e}");
            // A cloud preflight that fails is a network blip, not a broken setup.
            if primary && cfg.is_local() {
                readiness::set_speech(readiness::Stage::Failed { error: e.to_string() });
            }
        }
    }
}

/// Longest the warm request may take: a 2B model on a slow processor needs up to a
/// minute after loading; longer than this and something is wrong.
const WARM_COMPLETE: Duration = Duration::from_secs(90);

/// Warm a cleanup provider. Local models get one real (tiny) cleanup request so the
/// model, the JSON grammar and the cached system prompt are all ready; the first
/// real dictation otherwise pays several seconds. The model is loaded first, so the
/// timed request measures the model's speed (it sets how long cleanup waits for it).
async fn warm_llm(cfg: &app_core::ProviderConfig) {
    let a = app();
    let s = a.settings();
    let primary = s.llm_chain.first() == Some(&cfg.id);
    let Some(p) = a.llm_provider(cfg) else { return };
    if cfg.ptype == ProviderType::Ollama {
        let base = cfg.base_url.clone().unwrap_or_else(|| ollama::DEFAULT_BASE.into());
        let st = ollama::status(&base).await;
        readiness::observe_ollama(&base, &st, primary.then_some(cfg.model.as_str()), false);
        if !st.reachable || !st.models.contains(&cfg.model) {
            tracing::info!(provider = %cfg.id, model = %cfg.model, reachable = st.reachable, "cleanup model not available; nothing to warm");
            return;
        }
    }
    let t0 = std::time::Instant::now();
    let r: Result<(), String> = if cfg.is_local() {
        let req = app_cleanup::build_cleanup_request(app_core::CleanupLevel::Light, "", &Default::default(), &[], "um hello there this is a warm up");
        match p.warmup().await {
            Ok(()) => {
                let t1 = std::time::Instant::now();
                match tokio::time::timeout(WARM_COMPLETE, p.complete(&req)).await {
                    Ok(Ok(_)) => {
                        let ms = t1.elapsed().as_millis() as u64;
                        a.llm_warm_ms.lock().unwrap().insert(cfg.id.clone(), (cfg.model.clone(), ms));
                        tracing::info!(provider = %cfg.id, model = %cfg.model, ms, "cleanup model speed");
                        Ok(())
                    }
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err(format!("The cleanup model did not answer within {} seconds.", WARM_COMPLETE.as_secs())),
                }
            }
            Err(e) => Err(e.to_string()),
        }
    } else {
        p.warmup().await.map_err(|e| e.to_string())
    };
    match r {
        Ok(()) => {
            tracing::info!(provider = %cfg.id, ms = t0.elapsed().as_millis() as u64, "cleanup warm");
            if primary {
                readiness::set_cleanup(readiness::Stage::Ready);
            }
        }
        Err(e) => {
            tracing::warn!(provider = %cfg.id, "cleanup warmup failed: {e}");
            if primary && cfg.is_local() {
                readiness::set_cleanup(readiness::Stage::Failed { error: e });
            }
        }
    }
}

/// Parameter count in billions from an Ollama tag: `qwen3:4b` → 4, `gemma4:e2b-it-qat` → 2
/// (Gemma's "effective" size), `llama3.2:3b-instruct-q4_K_M` → 3. Unknown → 8.
fn tag_size_b(tag: &str) -> f64 {
    let t = tag.rsplit(':').next().unwrap_or(tag).split('-').next().unwrap_or("");
    let t = t.strip_prefix(['e', 'E']).unwrap_or(t);
    t.strip_suffix(['b', 'B']).and_then(|n| n.parse::<f64>().ok()).unwrap_or(8.0)
}

/// Lower is better. Prefers ~4B instruct models and the families that measured best in
/// measurements on real dictations (Gemma 4); Qwen3 and Granite 4.2 return lowercase, unpunctuated
/// text under the JSON grammar, and reasoning-only or very large models are slow.
fn ollama_pick_score(tag: &str) -> f64 {
    let l = tag.to_lowercase();
    let size = tag_size_b(tag);
    let mut s = (size - 4.0).abs();
    if l.contains("r1") || l.contains("reason") || l.contains("uncensored") || l.contains("embed") {
        s += 100.0;
    }
    if size > 14.0 {
        s += 20.0;
    }
    if ["qwen", "gemma", "llama", "mistral", "phi"].iter().any(|k| l.contains(k)) {
        s -= 1.0;
    }
    if l.starts_with("gemma4:") {
        s -= 2.0;
    }
    if l.starts_with("qwen3:") || l.starts_with("granite4") {
        s += 6.0;
    }
    s
}

fn pick_ollama_model(installed: &[String]) -> Option<String> {
    installed.iter().min_by(|x, y| ollama_pick_score(x).partial_cmp(&ollama_pick_score(y)).unwrap_or(std::cmp::Ordering::Equal)).cloned()
}

/// If the configured Ollama model is not installed, pick a suitable installed one
/// (small instruct models first; reasoning-only and very large models last).
async fn auto_pick_ollama_model() {
    let a = app();
    let Some(cfg) = a.settings.read().unwrap().provider("ollama").cloned() else { return };
    let base = cfg.base_url.clone().unwrap_or_else(|| "http://localhost:11434".into());
    let Ok(installed) = app_llm::Ollama::list_models(&base).await else { return };
    if installed.is_empty() || installed.iter().any(|m| m == &cfg.model) {
        return;
    }
    // The guided setup installs the shipped default; while that pull runs (or after a
    // restart in the middle of it) the choice must stay, not be swapped for whatever
    // else happens to be installed.
    if cfg.model == ollama::MODEL {
        tracing::info!(model = %cfg.model, "Ollama model not installed yet; keeping the default");
        return;
    }
    if let Some(m) = pick_ollama_model(&installed) {
        tracing::info!(from = %cfg.model, to = %m, "Ollama model not installed; using an installed one");
        let mut s = a.settings();
        if let Some(p) = s.providers.iter_mut().find(|p| p.id == "ollama") {
            p.model = m;
        }
        let _ = commands::apply_settings(s);
    }
}

/// Recordings left in sessions/ by a crash become History items that can be retried.
/// With history off they are deleted, as any other recording would have been.
fn recover_orphans() {
    let a = app();
    let keep = a.settings.read().unwrap().store_history;
    for path in history::orphaned_sessions(&a.paths.sessions()) {
        if !keep {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let _ = app_audio::wav::repair(&path);
        let Some(name) = path.file_name() else { continue };
        let dest = a.paths.audio().join(name);
        if std::fs::rename(&path, &dest).is_err() {
            continue;
        }
        let pcm_len = app_audio::wav::read(&dest).map(|p| p.len()).unwrap_or(0);
        if pcm_len < 3200 {
            let _ = std::fs::remove_file(&dest);
            continue;
        }
        let rec = HistoryRecord {
            id: History::new_id(),
            created_at: now_ms(),
            status: Status::Unprocessed,
            mode: "dictation".into(),
            audio_path: Some(name.to_string_lossy().into()),
            audio_ms: (pcm_len * 1000 / 16_000) as u64,
            error: Some("Recovered after an unexpected shutdown. Use Retry to transcribe it.".into()),
            ..Default::default()
        };
        let days = a.settings.read().unwrap().audio_retention_days;
        if let Some(Err(e)) = a.with_history(|h| h.upsert(&rec, days)) {
            tracing::error!("could not record the recovered recording: {e}");
        }
        tracing::warn!("recovered unprocessed recording {}", dest.display());
    }
}

fn periodic_tasks() {
    tauri::async_runtime::spawn(async {
        loop {
            let a = app();
            let purged = a.with_history(|h| h.purge_audio()).unwrap_or(0);
            if purged > 0 {
                tracing::info!(purged, "old audio deleted");
            }
            tokio::time::sleep(Duration::from_secs(20 * 60)).await;
            // Keep a local cleanup model loaded; the flight makes sure this never
            // overlaps a warm-up that is already running.
            let s = a.settings();
            if a.chain(&s.llm_chain).first().is_some_and(|cfg| cfg.ptype == ProviderType::Ollama) {
                warmup();
            }
        }
    });
}

/// Pause before hotkey hook attempt `n`: 1 s, doubling, at most 30 s.
fn hotkey_retry_delay(n: u32) -> Duration {
    Duration::from_secs((1u64 << n.min(5)).min(30))
}

/// Attempts after the first before the hook is given up for this session.
const HOTKEY_RETRIES: u32 = 10;

/// Install the keyboard hook and forward its events to `out`. On Windows the hook can
/// fail while the desktop is still coming up at login; it is then tried again with a
/// growing pause instead of leaving every shortcut dead until the next restart.
fn start_hotkeys(out: crossbeam_channel::Sender<(app_hotkey::HookEvent, std::time::Instant)>) {
    std::thread::Builder::new()
        .name("hotkey-start".into())
        .spawn(move || {
            let a = app();
            let mut attempt = 0u32;
            loop {
                let hotkeys = a.settings().hotkeys.clone();
                match app_hotkey::start(&hotkeys) {
                    Ok(rx) => {
                        if attempt > 0 {
                            tracing::info!(attempt, "hotkeys available now");
                            *a.hotkeys_ok.lock().unwrap() = Ok(());
                            pill::clear_badge("hotkeys_off");
                        }
                        watch_permissions();
                        for ev in rx {
                            if out.send(ev).is_err() {
                                return;
                            }
                        }
                        return;
                    }
                    Err(e) => {
                        if attempt == 0 {
                            tracing::error!("hotkeys unavailable: {e}");
                            *a.hotkeys_ok.lock().unwrap() = Err(e);
                            pill::set_badge(Some("hotkeys_off"), Some("Shortcuts are not available"));
                        } else {
                            tracing::warn!(attempt, "hotkeys still unavailable: {e}");
                        }
                        if attempt >= HOTKEY_RETRIES {
                            tracing::error!("giving up on the keyboard hook for this session");
                            return;
                        }
                        std::thread::sleep(hotkey_retry_delay(attempt));
                        attempt += 1;
                    }
                }
            }
        })
        .ok();
}

/// Whether to show the Accessibility system prompt at this launch: once, by hand.
/// At login the dialog would land on the desktop every morning for someone who
/// declined; the pill badge and the Permissions rows cover that case.
fn should_prompt_accessibility(login_launch: bool, granted: bool) -> bool {
    !granted && !login_launch
}

/// One line with everything the "it doesn't work after a restart" report needs.
fn self_check() {
    tauri::async_runtime::spawn(async {
        let a = app();
        let s = a.settings();
        let autostart = commands::autostart_state(&s);
        let speech = a.chain(&s.asr_chain).first().map(|c| format!("{}:{}", c.id, c.model));
        let ollama = readiness::primary_ollama(&s).map(|c| c.base_url.unwrap_or_else(|| ollama::DEFAULT_BASE.into()));
        let ollama_reachable = match &ollama {
            Some(base) => Some(ollama::reachable(base).await),
            None => None,
        };
        let p = app_platform::permissions();
        tracing::info!(
            exe = %std::env::current_exe().unwrap_or_default().display(),
            location = ?app_platform::app_location(),
            login_launch = app_platform::launched_at_login(),
            uptime_s = app_platform::uptime().as_secs(),
            autostart_wanted = autostart.wanted,
            autostart_active = autostart.active,
            autostart_detail = autostart.detail.as_deref().unwrap_or(""),
            speech = speech.as_deref().unwrap_or("none installed"),
            ollama_installed = ollama::installed(),
            ollama_reachable = ?ollama_reachable,
            hotkeys = a.hotkeys_ok.lock().unwrap().is_ok(),
            accessibility = p.accessibility,
            microphone = %p.microphone,
            "self-check"
        );
    });
}

/// macOS: shortcuts wait for the Accessibility permission. Show it on the pill until
/// it is granted (the event tap then installs itself without a restart).
fn watch_permissions() {
    if cfg!(not(target_os = "macos")) {
        return;
    }
    std::thread::Builder::new()
        .name("permissions".into())
        .spawn(|| {
            let mut shown = false;
            let mut last = None;
            loop {
                let p = app_platform::permissions();
                if last.as_ref() != Some(&p) {
                    tracing::info!(accessibility = p.accessibility, input_monitoring = p.input_monitoring, microphone = %p.microphone, "permissions");
                    // Ask once per manual launch: the system prompt also adds Dettino to the list
                    // in System Settings, so people who skipped onboarding only need to switch it on.
                    if last.is_none() && should_prompt_accessibility(app_platform::launched_at_login(), p.accessibility) {
                        app_platform::request_permission("accessibility");
                    }
                    last = Some(p.clone());
                }
                let missing = !p.accessibility;
                if missing != shown {
                    shown = missing;
                    if missing {
                        pill::set_badge(Some("hotkeys_off"), Some("Allow Accessibility in System Settings for shortcuts"));
                    } else if pill::current().badge.as_deref() == Some("hotkeys_off") {
                        pill::set_badge(None, None);
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .ok();
}

pub fn run() {
    // Before anything that creates the data folder.
    if uninstall::handle_args() {
        return;
    }
    let paths = Paths::default_root();
    let _ = paths.ensure();
    init_logging(&paths);
    // The first line of every log: which build this person runs.
    let build = build_info::current();
    tracing::info!(
        version = %build.label,
        commit = build.commit,
        dirty = build.dirty,
        commit_date = build.commit_date,
        profile = build.profile,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        exe = %std::env::current_exe().unwrap_or_default().display(),
        expires = ?expiry::date(),
        "starting"
    );
    // Before settings, the keychain, Tauri and Ollama: an expired test build only says so.
    if expiry::expired() {
        expiry::stop();
        return;
    }
    let mut settings = settings_store::load(&paths.settings());
    tracing::info!("settings loaded");
    // Pause is for "not right now", not across restarts: a pause left on from a
    // previous session would silently disable dictation on the next launch.
    if settings.paused {
        settings.paused = false;
        if let Err(e) = settings_store::save(&paths.settings(), &settings) {
            tracing::warn!("could not save settings: {e}");
        }
        tracing::info!("cleared pause left over from the previous session");
    }
    // macOS may show a Keychain prompt here (a new signature reading an older item); startup waits for it.
    tracing::info!("reading the history key from the keychain");
    let history = match secrets::history_key().and_then(|k| History::open(&paths.db(), k)) {
        Ok(h) => Some(h.with_audio_dir(paths.audio())),
        Err(e) => {
            tracing::error!("history unavailable: {e}");
            None
        }
    };

    tracing::info!(history = history.is_some(), "history opened");
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {
            commands::show_main(None);
        }))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_settings,
            commands::set_preset,
            commands::set_provider_key,
            commands::set_paused,
            commands::test_provider,
            commands::mic_check,
            commands::list_devices,
            commands::mic_monitor,
            commands::subscribe_levels,
            commands::pill_ready,
            commands::pill_rect,
            commands::pill_action,
            commands::pill_reset_position,
            commands::preview_sound,
            commands::hotkeys_suspend,
            commands::warm_now,
            commands::model_download,
            commands::free_disk_mb,
            commands::model_delete,
            commands::history_list,
            commands::history_delete,
            commands::history_delete_all,
            commands::history_audio,
            commands::history_retry,
            commands::copy_text,
            commands::suggestions_list,
            commands::suggestion_resolve,
            commands::preview_context,
            ollama::ollama_status,
            ollama::ollama_install,
            ollama::ollama_launch,
            ollama::ollama_pull,
        ollama::ollama_cancel,
            commands::finish_onboarding,
            commands::open_data_folder,
            commands::export_settings,
            commands::import_settings,
            commands::show_main,
            commands::accent,
            commands::permissions,
            commands::resolve_app,
            commands::request_permission,
            commands::open_permission_settings,
        ])
        .setup(move |tauri_app| {
            tracing::info!("setup start");
            let handle = tauri_app.handle().clone();
            // A menu bar app: no Dock icon until the settings window is opened.
            #[cfg(target_os = "macos")]
            tauri_app.set_activation_policy(if settings.onboarding_done {
                tauri::ActivationPolicy::Accessory
            } else {
                tauri::ActivationPolicy::Regular
            });
            let (audio, audio_rx) = AudioEngine::start(commands::startup_audio_config(&settings));
            let (ctl_tx, ctl_rx) = crossbeam_channel::unbounded();
            let (job_tx, job_rx) = tokio::sync::mpsc::unbounded_channel();
            let onboarding_done = settings.onboarding_done;
            let launch = settings.launch_at_login;
            App::init(handle.clone(), paths.clone(), settings.clone(), history, audio, ctl_tx, job_tx);
            install_vad();
            pipeline::start(job_rx);
            periodic_tasks();
            expiry::watch(handle.clone());

            // Launch at login is registered once the onboarding's last page has shown the
            // switch (finish_onboarding); until then the setting is only a setting.
            if onboarding_done {
                if let Err(e) = app_platform::set_launch_at_login(launch) {
                    tracing::warn!("launch at login: {e}");
                }
            }

            tracing::info!("creating windows");
            pill::create(&handle)?;
            // Onboarding runs in a compact 720×520 window; the settings window is larger.
            let (w, h, min_w, min_h) = if onboarding_done { (1040.0, 700.0, 820.0, 560.0) } else { (720.0, 520.0, 720.0, 520.0) };
            WebviewWindowBuilder::new(&handle, "main", WebviewUrl::App("index.html".into()))
                .title(APP_NAME)
                .inner_size(w, h)
                .min_inner_size(min_w, min_h)
                .center()
                .visible(!onboarding_done)
                .build()?;
            tray::create(&handle)?;

            tracing::info!("starting hotkeys");
            let (hk_tx, hk_rx) = crossbeam_channel::unbounded();
            start_hotkeys(hk_tx);
            std::thread::Builder::new()
                .name("controller".into())
                .spawn(move || controller::run(hk_rx, audio_rx, ctl_rx))?;
            pill::start_hover_thread();
            pill::place(None);
            startup::run();
            // Off the launch path: repairing a crashed recording can take a moment on a slow disk.
            std::thread::Builder::new().name("recover".into()).spawn(recover_orphans)?;
            self_check();
            tracing::info!("setup done");
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            ("main", WindowEvent::CloseRequested { api, .. }) => {
                // Closing the settings window keeps dictation running in the tray.
                api.prevent_close();
                commands::hide_main(window);
            }
            ("pill", WindowEvent::Moved(_)) => pill::on_moved(),
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running the app");
}

#[cfg(test)]
mod pick_tests {
    use super::*;

    #[test]
    fn the_hotkey_hook_is_retried_with_a_growing_pause() {
        let d: Vec<u64> = (0..6).map(|n| hotkey_retry_delay(n).as_secs()).collect();
        assert_eq!(d[0], 1);
        assert!(d.windows(2).all(|w| w[0] <= w[1]), "{d:?}");
        assert_eq!(d[5], 30, "capped at half a minute");
    }

    #[test]
    fn accessibility_is_asked_for_by_hand_not_at_login() {
        assert!(should_prompt_accessibility(false, false), "a manual launch without the permission asks once");
        assert!(!should_prompt_accessibility(true, false), "a login launch leaves the desktop alone");
        assert!(!should_prompt_accessibility(false, true), "nothing to ask once granted");
    }

    #[test]
    fn tag_sizes() {
        assert_eq!(tag_size_b("qwen3:4b"), 4.0);
        assert_eq!(tag_size_b("gemma4:e2b-it-qat"), 2.0);
        assert_eq!(tag_size_b("gemma4:e4b-it-qat"), 4.0);
        assert_eq!(tag_size_b("llama3.2:3b-instruct-q4_K_M"), 3.0);
        assert_eq!(tag_size_b("qwen3.8:27b"), 27.0);
        assert_eq!(tag_size_b("mystery:latest"), 8.0);
    }

    #[test]
    fn prefers_gemma4_over_the_old_defaults() {
        let installed: Vec<String> =
            ["qwen3:4b", "qwen3.5:9b", "qwen3.8:27b", "deepseek-r1:8b", "gemma4:e2b-it-qat", "gemma4:e4b-it-qat"].map(String::from).into();
        assert_eq!(pick_ollama_model(&installed).as_deref(), Some("gemma4:e4b-it-qat"));
        let old: Vec<String> = ["qwen3:4b", "qwen3.5:9b", "deepseek-r1:8b", "qwen3.8:27b"].map(String::from).into();
        assert_eq!(pick_ollama_model(&old).as_deref(), Some("qwen3.5:9b"), "qwen3 is a last resort among instruct models");
        assert_eq!(pick_ollama_model(&["deepseek-r1:8b".to_string(), "qwen3:4b".to_string()]).as_deref(), Some("qwen3:4b"));
    }
}
