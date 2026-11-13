//! Shared application state and provider construction.

use app_asr_local::{ParakeetProvider, WhisperOnnxProvider};
use app_audio::AudioEngine;
use app_core::{AsrProvider, LlmProvider, ProviderConfig, ProviderStats, ProviderType, Settings};
use app_storage::{secrets, History, Paths};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;
use tauri::AppHandle;

use crate::controller::CtlMsg;
use crate::pipeline::Job;

pub struct LastPaste {
    pub record_id: String,
    pub raw: String,
    pub final_text: String,
    pub showing_raw: bool,
    pub at: Instant,
    pub user_keys: u64,
}

pub struct App {
    pub handle: AppHandle,
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub history: Mutex<Option<History>>,
    pub stats: Mutex<ProviderStats>,
    /// Last measured warm cleanup latency of on-device LLMs: id → (model, ms).
    pub llm_warm_ms: Mutex<HashMap<String, (String, u64)>>,
    pub audio: AudioEngine,
    pub ctl: crossbeam_channel::Sender<CtlMsg>,
    pub jobs: tokio::sync::mpsc::UnboundedSender<Job>,
    key_cache: Mutex<HashMap<String, Option<String>>>,
    local_asr: Mutex<HashMap<String, Arc<dyn AsrProvider>>>,
    pub last_paste: Mutex<Option<LastPaste>>,
    /// Text that could not be pasted, waiting for the user.
    pub held: Mutex<Option<(String, String)>>, // (record id, text)
    /// Last cancelled take, restorable from the pill.
    pub cancelled: Mutex<Option<String>>, // record id
    pub hotkeys_ok: Mutex<Result<(), String>>,
    pub allow_until: Mutex<HashMap<String, Instant>>,
}

static APP: OnceLock<Arc<App>> = OnceLock::new();

pub fn app() -> &'static Arc<App> {
    APP.get().expect("app initialized")
}

pub fn try_app() -> Option<&'static Arc<App>> {
    APP.get()
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        handle: AppHandle,
        paths: Paths,
        settings: Settings,
        history: Option<History>,
        audio: AudioEngine,
        ctl: crossbeam_channel::Sender<CtlMsg>,
        jobs: tokio::sync::mpsc::UnboundedSender<Job>,
    ) -> Arc<App> {
        let stats = history.as_ref().map(|h| h.load_stats()).unwrap_or_default();
        let app = Arc::new(App {
            handle,
            paths,
            settings: RwLock::new(settings),
            history: Mutex::new(history),
            stats: Mutex::new(stats),
            llm_warm_ms: Mutex::new(HashMap::new()),
            audio,
            ctl,
            jobs,
            key_cache: Mutex::new(HashMap::new()),
            local_asr: Mutex::new(HashMap::new()),
            last_paste: Mutex::new(None),
            held: Mutex::new(None),
            cancelled: Mutex::new(None),
            hotkeys_ok: Mutex::new(Ok(())),
            allow_until: Mutex::new(HashMap::new()),
        });
        let _ = APP.set(app.clone());
        app
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn key(&self, provider_id: &str) -> Option<String> {
        let mut cache = self.key_cache.lock().unwrap();
        if let Some(v) = cache.get(provider_id) {
            return v.clone();
        }
        // Only a definite answer is cached: a keychain that cannot be asked right now
        // must not turn into "no key" for the rest of the session.
        match secrets::provider_key(provider_id) {
            Ok(v) => {
                cache.insert(provider_id.to_string(), v.clone());
                v
            }
            Err(e) => {
                tracing::warn!(provider = provider_id, "keychain unavailable: {e}");
                None
            }
        }
    }

    pub fn set_key(&self, provider_id: &str, key: &str) -> Result<(), String> {
        secrets::set(&secrets::provider_key_name(provider_id), key)?;
        let v = if key.trim().is_empty() { None } else { Some(key.trim().to_string()) };
        self.key_cache.lock().unwrap().insert(provider_id.to_string(), v);
        Ok(())
    }

    pub fn has_key(&self, provider_id: &str) -> bool {
        self.key(provider_id).is_some()
    }

    fn model_dir_for(&self, cfg: &ProviderConfig) -> std::path::PathBuf {
        let models = self.paths.models();
        match cfg.ptype {
            ProviderType::ParakeetLocal => {
                if cfg.model.contains("v3") {
                    models.join("parakeet-v3")
                } else {
                    models.join("parakeet")
                }
            }
            ProviderType::WhisperLocal => {
                if cfg.model.contains("small") {
                    models.join("whisper-small")
                } else {
                    models.join("whisper")
                }
            }
            _ => models,
        }
    }

    /// Whether a local model for this provider is on disk.
    pub fn local_installed(&self, cfg: &ProviderConfig) -> bool {
        let dir = self.model_dir_for(cfg);
        match cfg.ptype {
            ProviderType::ParakeetLocal => ParakeetProvider::new(&cfg.id, dir).is_installed(),
            ProviderType::WhisperLocal => WhisperOnnxProvider::new(&cfg.id, dir).is_installed(),
            _ => true,
        }
    }

    pub fn asr_provider(&self, cfg: &ProviderConfig) -> Option<Arc<dyn AsrProvider>> {
        match cfg.ptype {
            ProviderType::ParakeetLocal | ProviderType::WhisperLocal => {
                let cache_key = format!("{}:{}", cfg.id, cfg.model);
                let mut cache = self.local_asr.lock().unwrap();
                if let Some(p) = cache.get(&cache_key) {
                    return Some(p.clone());
                }
                let dir = self.model_dir_for(cfg);
                let p: Arc<dyn AsrProvider> = if cfg.ptype == ProviderType::ParakeetLocal {
                    Arc::new(ParakeetProvider::new(&cfg.id, dir))
                } else {
                    Arc::new(WhisperOnnxProvider::new(&cfg.id, dir))
                };
                cache.insert(cache_key, p.clone());
                Some(p)
            }
            _ => app_asr_cloud::build(cfg, self.key(&cfg.id)),
        }
    }

    pub fn llm_provider(&self, cfg: &ProviderConfig) -> Option<Arc<dyn LlmProvider>> {
        app_llm::build(cfg, self.key(&cfg.id))
    }

    /// Enabled providers in a chain, skipping ones that cannot work (missing key / model).
    pub fn chain(&self, ids: &[String]) -> Vec<ProviderConfig> {
        let s = self.settings.read().unwrap();
        ids.iter()
            .filter_map(|id| s.provider(id).cloned())
            .filter(|p| p.enabled)
            .filter(|p| !p.ptype.needs_key() || self.has_key(&p.id))
            .filter(|p| !matches!(p.ptype, ProviderType::ParakeetLocal | ProviderType::WhisperLocal) || self.local_installed(p))
            .collect()
    }

    pub fn with_history<R>(&self, f: impl FnOnce(&History) -> R) -> Option<R> {
        self.history.lock().unwrap().as_ref().map(f)
    }

    pub fn record_stat(&self, provider: &str, latency_ms: u64, ok: bool, err: Option<String>) {
        let ts = now_ms() as u64;
        self.stats.lock().unwrap().record(
            provider,
            app_core::stats::Sample { ts, latency_ms, ok, err: err.clone() },
        );
        self.with_history(|h| h.record_stat(provider, latency_ms, ok, err.as_deref()));
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
