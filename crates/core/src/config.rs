//! Persisted settings model. Everything has a default so older settings files
//! keep loading after new fields are added.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::keys::Hotkeys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CleanupLevel {
    Off,
    #[default]
    Light,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// The default: on-device transcription runs on any CPU, and the onboarding
    /// sets up Ollama for cleanup, so a fresh install never needs an API key.
    #[default]
    Private,
    /// Shown as "Cloud" in the app. The serialized name stays `fast` so saved settings keep working.
    Fast,
    Balanced,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MicMode {
    /// Stream open while the app runs; pre-roll always available.
    #[default]
    AlwaysWarm,
    /// Open when a chord modifier goes down; keep open 120 s.
    WarmOnModifier,
    /// Open when a dictation starts (or earlier, see [`MicEarlyOpen`]) and close 3 s
    /// after the last use: the microphone (and macOS's orange indicator) is on only
    /// around a dictation. Loses the first syllable unless opened early.
    OnDemand,
}

/// Whether `OnDemand` opens the microphone when a chord modifier goes down, before
/// the chord completes. Opening early saves the first syllable but lights the mic
/// indicator on every Ctrl+C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MicEarlyOpen {
    /// Open only once the chord is complete.
    #[default]
    Off,
    /// Open on modifier-down; close as soon as the modifier turns out to be part of
    /// another shortcut or is released alone.
    CloseOnOtherKey,
    /// Open on modifier-down and keep open 3 s.
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Asr,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    WhisperLocal,
    ParakeetLocal,
    OpenaiBatch,
    OpenaiCompatAsr,
    Anthropic,
    OpenaiChat,
    PerplexityAgent,
    Ollama,
    OpenaiCompatChat,
}

impl ProviderType {
    pub fn kind(self) -> ProviderKind {
        match self {
            Self::WhisperLocal | Self::ParakeetLocal | Self::OpenaiBatch | Self::OpenaiCompatAsr => {
                ProviderKind::Asr
            }
            _ => ProviderKind::Llm,
        }
    }

    /// Whether audio or text stays on this computer.
    pub fn is_local(self) -> bool {
        matches!(self, Self::WhisperLocal | Self::ParakeetLocal | Self::Ollama)
    }

    pub fn needs_key(self) -> bool {
        matches!(
            self,
            Self::OpenaiBatch | Self::Anthropic | Self::OpenaiChat | Self::PerplexityAgent
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub ptype: ProviderType,
    pub display_name: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: String,
    /// Perplexity: ordered fallback models; others ignore it.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Custom endpoints: "server accepts a prompt".
    #[serde(default)]
    pub accepts_prompt: bool,
    /// Custom/local endpoints may be treated as local (e.g. a LAN server).
    #[serde(default)]
    pub treat_as_local: bool,
}

fn yes() -> bool {
    true
}

impl ProviderConfig {
    pub fn kind(&self) -> ProviderKind {
        self.ptype.kind()
    }
    pub fn is_local(&self) -> bool {
        self.ptype.is_local() || self.treat_as_local
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSettings {
    pub enabled: bool,
    pub app_name: bool,
    pub window_title: bool,
    pub caret_text: bool,
    pub selection: bool,
    pub visible_text: bool,
    /// Only send context to providers that run on this computer.
    pub local_only: bool,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            app_name: true,
            window_title: true,
            caret_text: true,
            selection: true,
            visible_text: true,
            local_only: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InsertMode {
    #[default]
    Paste,
    Type,
}

/// Which look the pill window renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PillStyle {
    /// The original: a grey idle dot that grows into a rounded pill with glyph, waveform
    /// and label, plus an icon tray on hover. Window 420 × 124.
    Classic,
    /// "One line" (decided 2026-10-18, the default since 2026-10-20): a single line that is
    /// the idle mark, the meter, the thinking indicator and the success mark; words sit on it.
    /// Window 360 × 80.
    #[default]
    Line,
}

/// The voice of the start / stop / hands-free / error sounds. Synthesised in
/// `crates/platform/src/tones.rs`; when they play does not depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SoundStyle {
    /// The original sine blips, 880 → 1175 Hz.
    #[default]
    Classic,
    /// Classic pitches, overlapping notes that fade out, with a little octave.
    SoftPluck,
    /// Lower and warmer (E5 → A5), marimba-like with a soft tick.
    Mallet,
    /// Higher and bell-like (A5 → E6), with a longer ring.
    Glass,
    /// Low and round (D5 → G5), with a tiny pitch drop on each note.
    WarmPop,
}

/// The accent palette for buttons, toggles and the pill (Seafoam keeps the listening pill blue). The hex values live in
/// `apps/desktop/src/styles/tokens.css` and `src/lib/theme.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    /// The brand colour and the default since 2026-11-14: seafoam buttons with near-black text.
    /// The pill still listens in blue and flashes seafoam after a paste.
    #[default]
    Surf,
    /// Blue; the default from 2026-10-20 to 2026-11-14.
    Cobalt,
    Ultramarine,
    Indigo,
    /// Teal, after Olivetti's Lettera typewriters.
    Lettera,
    Plum,
    Orchid,
    Graphite,
    /// Olivetti red, the default from 2026-10-19 to 2026-10-20.
    Red,
    /// Follow the Windows/macOS accent colour, clamped to AA by the UI.
    System,
}

/// How the One line pill shows that a paste landed. Classic always shows a check and "Pasted".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SuccessAnimation {
    /// The line turns green and swells for a moment. No mark, no word.
    #[default]
    Flash,
    /// The line bends into a check mark.
    Check,
    /// The check mark plus the word "Pasted".
    CheckWord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRule {
    /// Lower-case executable name (Windows) or bundle id (macOS), e.g. "windowsterminal.exe".
    pub app: String,
    #[serde(default)]
    pub cleanup: Option<CleanupLevel>,
    #[serde(default)]
    pub context: Option<bool>,
    #[serde(default)]
    pub never_dictate: bool,
    #[serde(default)]
    pub insert_mode: Option<InsertMode>,
    #[serde(default)]
    pub paste_delay_ms: Option<u64>,
    /// Paste chord override: "ctrl+v" (default), "ctrl+shift+v", "shift+insert".
    #[serde(default)]
    pub paste_chord: Option<String>,
}

pub fn default_app_rules() -> Vec<AppRule> {
    let terminal = |app: &str| AppRule {
        app: app.into(),
        cleanup: Some(CleanupLevel::Off),
        context: Some(false),
        never_dictate: false,
        insert_mode: None,
        paste_delay_ms: Some(400),
        paste_chord: None,
    };
    let never = |app: &str| AppRule {
        app: app.into(),
        cleanup: None,
        context: Some(false),
        never_dictate: true,
        insert_mode: None,
        paste_delay_ms: None,
        paste_chord: None,
    };
    vec![
        terminal("windowsterminal.exe"),
        terminal("conhost.exe"),
        terminal("openconsole.exe"),
        terminal("wezterm-gui.exe"),
        terminal("alacritty.exe"),
        terminal("powershell.exe"),
        terminal("pwsh.exe"),
        terminal("cmd.exe"),
        terminal("com.apple.terminal"),
        terminal("com.googlecode.iterm2"),
        terminal("com.mitchellh.ghostty"),
        terminal("net.kovidgoyal.kitty"),
        terminal("dev.warp.warp-stable"),
        terminal("com.github.wez.wezterm"),
        terminal("org.alacritty"),
        AppRule {
            app: "mstsc.exe".into(),
            cleanup: None,
            context: Some(false),
            never_dictate: false,
            insert_mode: None,
            paste_delay_ms: Some(600),
            paste_chord: None,
        },
        never("1password.exe"),
        never("bitwarden.exe"),
        never("keepass.exe"),
        never("keepassxc.exe"),
        never("com.1password.1password"),
        never("com.bitwarden.desktop"),
        never("com.apple.passwords"),
        never("com.apple.keychainaccess"),
        never("org.keepassxc.keepassxc"),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snippet {
    pub trigger: String,
    pub body: String,
    pub enabled: bool,
}

impl Default for Snippet {
    fn default() -> Self {
        Self { trigger: String::new(), body: String::new(), enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DictionaryEntry {
    pub term: String,
    /// Optional misspelling that should be replaced by `term`.
    pub replace_from: Option<String>,
    /// "manual" or "auto" (accepted suggestion).
    pub source: String,
    pub enabled: bool,
}

impl Default for DictionaryEntry {
    fn default() -> Self {
        Self { term: String::new(), replace_from: None, source: "manual".into(), enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PillPosition {
    pub dx: f64,
    pub dy: f64,
}

impl Default for PillPosition {
    fn default() -> Self {
        Self { dx: 0.0, dy: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub preset: Preset,
    pub providers: Vec<ProviderConfig>,
    /// Ordered provider ids.
    pub asr_chain: Vec<String>,
    pub llm_chain: Vec<String>,
    /// Hedge override in ms; `None` = adaptive max(1200, 1.5 × p95).
    pub hedge_ms: Option<u64>,
    /// Longest wait for cleanup in ms, any provider; `None` = adaptive (on-device:
    /// from the model's measured speed and the request size; cloud: from the request size).
    pub cleanup_wait_ms: Option<u64>,

    pub cleanup_level: CleanupLevel,
    pub standing_instruction: String,
    /// Spoken language as an ISO 639-1 code ("de"); `None` lets Whisper and cloud
    /// transcription detect it. Parakeet v2 understands English whatever this says.
    pub language: Option<String>,

    pub hotkeys: Hotkeys,
    pub tap_max_ms: u64,
    pub double_tap_handsfree: bool,
    pub handsfree_max_min: u64,

    pub mic_mode: MicMode,
    pub mic_early_open: MicEarlyOpen,
    pub input_device: Option<String>,
    pub preroll_ms: u64,
    pub prefer_wired: bool,

    pub context: ContextSettings,
    pub app_rules: Vec<AppRule>,
    pub dictionary: Vec<DictionaryEntry>,
    pub learn_from_corrections: bool,
    pub snippets: Vec<Snippet>,

    pub keep_on_clipboard: bool,
    pub sounds: bool,
    pub sound_volume: f32,
    pub sound_style: SoundStyle,
    pub pill_style: PillStyle,
    pub success_animation: SuccessAnimation,
    pub accent: Accent,
    pub launch_at_login: bool,
    /// Start a local Ollama in the background when Dettino starts (and again if it
    /// stops answering) whenever it is in the cleanup chain.
    pub start_ollama: bool,
    pub paused: bool,
    pub store_history: bool,
    pub audio_retention_days: u32,
    pub onboarding_done: bool,
    pub pill_positions: BTreeMap<String, PillPosition>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            preset: Preset::Private,
            providers: default_providers(),
            asr_chain: preset_chains(Preset::Private).map(|c| c.0).unwrap_or_default(),
            llm_chain: preset_chains(Preset::Private).map(|c| c.1).unwrap_or_default(),
            hedge_ms: None,
            cleanup_wait_ms: None,
            cleanup_level: CleanupLevel::Light,
            standing_instruction: String::new(),
            language: None,
            hotkeys: Hotkeys::default(),
            tap_max_ms: 250,
            double_tap_handsfree: false,
            handsfree_max_min: 20,
            // macOS shows an indicator whenever the microphone is open; an always-open
            // stream would light it permanently.
            mic_mode: if cfg!(target_os = "macos") { MicMode::OnDemand } else { MicMode::AlwaysWarm },
            mic_early_open: MicEarlyOpen::Off,
            input_device: None,
            preroll_ms: 1500,
            prefer_wired: true,
            context: ContextSettings::default(),
            app_rules: default_app_rules(),
            dictionary: Vec::new(),
            learn_from_corrections: true,
            snippets: Vec::new(),
            keep_on_clipboard: false,
            sounds: true,
            sound_volume: 0.3,
            sound_style: SoundStyle::Classic,
            pill_style: PillStyle::Line,
            success_animation: SuccessAnimation::Flash,
            accent: Accent::Surf,
            launch_at_login: true,
            start_ollama: true,
            paused: false,
            store_history: true,
            audio_retention_days: 7,
            onboarding_done: false,
            pill_positions: BTreeMap::new(),
        }
    }
}

pub fn default_providers() -> Vec<ProviderConfig> {
    let p = |id: &str, ptype, name: &str, base: Option<&str>, model: &str| ProviderConfig {
        id: id.into(),
        ptype,
        display_name: name.into(),
        base_url: base.map(Into::into),
        model: model.into(),
        models: Vec::new(),
        enabled: true,
        accepts_prompt: false,
        treat_as_local: false,
    };
    let mut perplexity = p(
        "perplexity",
        ProviderType::PerplexityAgent,
        "Perplexity",
        Some("https://api.perplexity.ai"),
        "anthropic/claude-haiku-5-5",
    );
    perplexity.models = vec![
        "anthropic/claude-haiku-5-5".into(),
        "openai/gpt-5.5".into(),
        "perplexity/sonar".into(),
    ];
    let mut lmstudio = p(
        "lmstudio",
        ProviderType::OpenaiCompatChat,
        "LM Studio (on this computer)",
        Some("http://localhost:1234/v1"),
        "local-model",
    );
    lmstudio.treat_as_local = true;
    vec![
        p("parakeet", ProviderType::ParakeetLocal, "Parakeet (on this computer)", None, "parakeet-tdt-0.6b-v2-int8"),
        p("whisper", ProviderType::WhisperLocal, "Whisper (on this computer)", None, "whisper-turbo"),
        p("openai-asr", ProviderType::OpenaiBatch, "OpenAI transcription", Some("https://api.openai.com/v1"), "gpt-4o-transcribe"),
        p("custom-asr", ProviderType::OpenaiCompatAsr, "Custom transcription server", Some("http://localhost:8000/v1"), "whisper-1"),
        p("anthropic", ProviderType::Anthropic, "Anthropic Claude", Some("https://api.anthropic.com"), "claude-haiku-5-5"),
        p("openai-chat", ProviderType::OpenaiChat, "OpenAI", Some("https://api.openai.com/v1"), "gpt-5-mini"),
        perplexity,
        p("ollama", ProviderType::Ollama, "Ollama (on this computer)", Some("http://localhost:11434"), "gemma4:e2b-it-qat"),
        lmstudio,
    ]
}

/// Chains for a preset. Private never contains a cloud provider.
pub fn preset_chains(preset: Preset) -> Option<(Vec<String>, Vec<String>)> {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match preset {
        Preset::Private => Some((v(&["parakeet", "whisper"]), v(&["ollama", "lmstudio"]))),
        Preset::Fast => Some((v(&["openai-asr", "custom-asr"]), v(&["anthropic", "perplexity", "openai-chat"]))),
        Preset::Balanced => Some((v(&["parakeet", "whisper", "openai-asr"]), v(&["anthropic", "ollama"]))),
        Preset::Custom => None,
    }
}

impl Settings {
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Pull every numeric setting into its working range (a hand-edited file or an
    /// older build may have written anything).
    pub fn clamp_ranges(&mut self) {
        self.sound_volume = if self.sound_volume.is_finite() { self.sound_volume.clamp(0.0, 1.0) } else { 0.3 };
        self.tap_max_ms = self.tap_max_ms.clamp(120, 600);
        self.handsfree_max_min = self.handsfree_max_min.clamp(1, 120);
        self.preroll_ms = self.preroll_ms.min(2000);
        self.audio_retention_days = self.audio_retention_days.clamp(1, 30);
        if let Some(ms) = self.cleanup_wait_ms.as_mut() {
            *ms = (*ms).clamp(500, 120_000);
        }
        if let Some(ms) = self.hedge_ms.as_mut() {
            *ms = (*ms).clamp(100, 60_000);
        }
    }

    /// Effective app rule for an app identifier (case-insensitive match).
    pub fn rule_for(&self, app: &str) -> Option<&AppRule> {
        let app = app.to_ascii_lowercase();
        self.app_rules.iter().find(|r| r.app.eq_ignore_ascii_case(&app))
    }

    pub fn apply_preset(&mut self, preset: Preset) {
        self.preset = preset;
        if let Some((asr, llm)) = preset_chains(preset) {
            self.asr_chain = asr;
            self.llm_chain = llm;
        }
    }

    /// [`Self::apply_preset`], then let Cloud (`Fast`) follow the keys already saved.
    pub fn apply_preset_with_keys(&mut self, preset: Preset, has_key: impl Fn(&str) -> bool) {
        let keep_cleanup = (self.preset == Preset::Fast && preset == Preset::Fast).then(|| self.llm_chain.clone());
        self.apply_preset(preset);
        // Re-picking Cloud keeps the cleanup provider the user chose for it.
        if let Some(llm) = keep_cleanup {
            self.llm_chain = llm;
        }
        self.follow_keys(has_key);
    }

    /// In Cloud mode, if the first cleanup provider has no key, move the ones that
    /// do to the front (in their preset order), so a single OpenAI key is enough for
    /// transcription and cleanup.
    pub fn follow_keys(&mut self, has_key: impl Fn(&str) -> bool) {
        if self.preset != Preset::Fast || self.llm_chain.first().is_none_or(|id| has_key(id)) {
            return;
        }
        self.llm_chain.sort_by_key(|id| !has_key(id));
    }

    /// Whether the current chains contain only on-device providers.
    pub fn is_fully_local(&self) -> bool {
        self.asr_chain
            .iter()
            .chain(self.llm_chain.iter())
            .filter_map(|id| self.provider(id))
            .all(|p| p.is_local())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_private_and_fully_local() {
        let s = Settings::default();
        assert_eq!(s.preset, Preset::Private);
        assert!(s.is_fully_local());
        assert_eq!(s.asr_chain[0], "parakeet");
        assert_eq!(s.llm_chain[0], "ollama");
    }

    #[test]
    fn private_preset_is_local() {
        let mut s = Settings::default();
        s.apply_preset(Preset::Private);
        assert!(s.is_fully_local());
        s.apply_preset(Preset::Fast);
        assert!(!s.is_fully_local());
    }

    #[test]
    fn fast_cleanup_follows_saved_keys() {
        let mut s = Settings::default();
        // Only an OpenAI key: OpenAI cleans up too.
        s.apply_preset_with_keys(Preset::Fast, |id| id.starts_with("openai"));
        assert_eq!(s.llm_chain, ["openai-chat", "anthropic", "perplexity"]);
        // An Anthropic key added later does not override that choice.
        s.follow_keys(|_| true);
        assert_eq!(s.llm_chain[0], "openai-chat");
        // Both keys from the start: the preset order wins.
        s.apply_preset_with_keys(Preset::Balanced, |_| true);
        s.apply_preset_with_keys(Preset::Fast, |_| true);
        assert_eq!(s.llm_chain[0], "anthropic");
        // The user picks OpenAI, then clicks Cloud again: the pick is kept.
        s.llm_chain = vec!["openai-chat".into(), "anthropic".into(), "perplexity".into()];
        s.apply_preset_with_keys(Preset::Fast, |_| true);
        assert_eq!(s.llm_chain[0], "openai-chat");
        // Other presets are never reordered.
        s.apply_preset_with_keys(Preset::Balanced, |id| id == "ollama");
        assert_eq!(s.llm_chain, ["anthropic", "ollama"]);
    }

    #[test]
    fn settings_roundtrip_and_missing_fields() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        let partial: Settings = serde_json::from_str(r#"{"tap_max_ms": 300}"#).unwrap();
        assert_eq!(partial.tap_max_ms, 300);
        assert_eq!(partial.preroll_ms, 1500);
        assert_eq!(partial.mic_early_open, MicEarlyOpen::Off);
        // Settings files without a saved pill style get the One line.
        assert_eq!(partial.pill_style, PillStyle::Line);
        assert_eq!(partial.success_animation, SuccessAnimation::Flash);
        assert_eq!(partial.accent, Accent::Surf);
        assert_eq!(partial.sound_style, SoundStyle::Classic);
        let line: Settings =
            serde_json::from_str(r#"{"pill_style": "classic", "success_animation": "check_word", "sound_style": "warm_pop"}"#).unwrap();
        assert_eq!(line.pill_style, PillStyle::Classic);
        assert_eq!(line.success_animation, SuccessAnimation::CheckWord);
        assert_eq!(line.sound_style, SoundStyle::WarmPop);
    }

    #[test]
    fn ranges_are_clamped() {
        let mut s = Settings { sound_volume: 7.0, tap_max_ms: 5, handsfree_max_min: 0, audio_retention_days: 0, ..Settings::default() };
        s.clamp_ranges();
        assert_eq!((s.sound_volume, s.tap_max_ms, s.handsfree_max_min, s.audio_retention_days), (1.0, 120, 1, 1));
        let mut ok = Settings::default();
        let before = ok.clone();
        ok.clamp_ranges();
        assert_eq!(ok, before, "defaults are already in range");
    }

    #[test]
    fn rule_lookup_case_insensitive() {
        let s = Settings::default();
        assert!(s.rule_for("WindowsTerminal.exe").is_some());
        assert!(s.rule_for("org.keepassxc.keepassxc").unwrap().never_dictate);
        assert!(s.rule_for("1Password.exe").unwrap().never_dictate);
        assert!(s.rule_for("notepad.exe").is_none());
    }

    #[test]
    fn chains_reference_existing_providers() {
        for preset in [Preset::Private, Preset::Fast, Preset::Balanced] {
            let mut s = Settings::default();
            s.apply_preset(preset);
            for id in s.asr_chain.iter().chain(s.llm_chain.iter()) {
                assert!(s.provider(id).is_some(), "{id} missing");
            }
        }
    }
}

#[cfg(test)]
mod fixture {
    /// `cargo test -p app-core dump_default_settings -- --ignored` writes the default
    /// settings for the UI's mock bridge (apps/desktop/src/dev/defaults.json).
    #[test]
    #[ignore]
    fn dump_default_settings() {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/dev/defaults.json");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(out, serde_json::to_string_pretty(&super::Settings::default()).unwrap()).unwrap();
    }
}
