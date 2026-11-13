//! OS integration: foreground app detection, screen context (UI Automation /
//! Accessibility), clipboard paste with verification and restore, typing.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForegroundInfo {
    pub hwnd: isize,
    /// Focused child window (Win32 focus), used for the paste-target check.
    pub focus_hwnd: isize,
    pub pid: u32,
    /// What app rules match: the lower-case executable name on Windows ("slack.exe"),
    /// the lower-case bundle identifier on macOS ("com.tinyspeck.slackmacgap").
    pub app: String,
    /// The app's name as people know it ("Slack"); empty when unknown.
    #[serde(default)]
    pub name: String,
    pub title: String,
    /// Target runs elevated while we do not: input injection is blocked (UIPI).
    pub blocked_elevated: bool,
    pub is_self: bool,
    /// Window rectangle in physical pixels: left, top, right, bottom.
    pub rect: (i32, i32, i32, i32),
}

impl ForegroundInfo {
    /// Name for messages: the app's own name, else one derived from `app`.
    pub fn display_name(&self) -> String {
        if self.name.is_empty() {
            display_app_name(&self.app)
        } else {
            self.name.clone()
        }
    }

    /// Same window and same focused control as when the dictation started.
    pub fn same_target(&self, other: &ForegroundInfo, window_only: bool) -> bool {
        self.hwnd == other.hwnd && (window_only || self.focus_hwnd == other.focus_hwnd)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ContextRequest {
    pub caret_text: bool,
    pub selection: bool,
    pub visible_text: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    pub control: Option<String>,
    pub is_password: bool,
    pub is_address_bar: bool,
    pub before: Option<String>,
    pub after: Option<String>,
    pub selection: Option<String>,
    pub visible_text: Option<String>,
    pub url: Option<String>,
    pub ms: u64,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteChord {
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

impl PasteChord {
    pub fn parse(s: Option<&str>) -> Self {
        match s.map(|s| s.to_ascii_lowercase().replace(' ', "")) {
            Some(ref s) if s == "ctrl+shift+v" => Self::CtrlShiftV,
            Some(ref s) if s == "shift+insert" => Self::ShiftInsert,
            _ => Self::CtrlV,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PasteRequest {
    pub text: String,
    pub chord: PasteChord,
    /// Type as Unicode key events instead of using the clipboard.
    pub type_instead: bool,
    pub restore_clipboard: bool,
    /// Wait after the target read the clipboard before restoring.
    pub restore_delay: Duration,
    /// Process the text goes to: only its clipboard read verifies the paste (Windows).
    pub target_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteOutcome {
    /// The target read our clipboard data. `restored` is None when restore was not requested.
    Verified { restored: Option<bool> },
    /// Text was typed as key events.
    Typed,
    /// Ctrl+V was sent but the target never read the clipboard; the text stays on it.
    Unverified,
    Failed(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
    pub vendor: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Start,
    Stop,
    HandsFree,
    Error,
}

/// Privacy permissions Dettino needs on macOS. Other systems report them granted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Permissions {
    /// Event tap, paste and screen context (Privacy & Security → Accessibility).
    pub accessibility: bool,
    /// Keyboard monitoring (Privacy & Security → Input Monitoring).
    pub input_monitoring: bool,
    /// granted | denied | undetermined | restricted | unknown
    pub microphone: String,
}

#[cfg(not(target_os = "macos"))]
pub fn permissions() -> Permissions {
    Permissions { accessibility: true, input_monitoring: true, microphone: "unknown".into() }
}

#[cfg(not(target_os = "macos"))]
pub fn request_permission(_kind: &str) {}

/// Where the running app lives. Only macOS cares: launch at login and the privacy
/// permissions are tied to the bundle's path, so an app run from the disk image, from
/// Downloads or through Gatekeeper's App Translocation loses both at the next login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppLocation {
    /// In /Applications or ~/Applications (or not an app bundle at all).
    Applications,
    /// Gatekeeper runs a copy from a random read-only folder; the path changes every launch.
    Translocated,
    /// A bundle somewhere else: Downloads, the Desktop, a mounted disk image.
    Elsewhere,
}

/// Classify an executable path (`home` is the user's home folder, when known).
pub fn app_location_of(exe: &std::path::Path, home: Option<&std::path::Path>) -> AppLocation {
    let text = exe.to_string_lossy();
    if text.contains("/AppTranslocation/") {
        return AppLocation::Translocated;
    }
    // The bundle is the nearest ancestor ending in ".app"; without one this is a bare
    // binary (cargo run, tauri dev), which no advice applies to.
    let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) else { return AppLocation::Applications };
    let Some(parent) = bundle.parent() else { return AppLocation::Elsewhere };
    if parent == std::path::Path::new("/Applications") || home.is_some_and(|h| parent == h.join("Applications")) {
        AppLocation::Applications
    } else {
        AppLocation::Elsewhere
    }
}

/// Where this process runs from (see [`AppLocation`]).
pub fn app_location() -> AppLocation {
    if !cfg!(target_os = "macos") {
        return AppLocation::Applications;
    }
    let Ok(exe) = std::env::current_exe() else { return AppLocation::Applications };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    app_location_of(&exe, home.as_deref())
}

/// Windows keeps Task Manager's Startup-apps switch in a 12-byte blob under
/// `Explorer\StartupApproved\Run`: first byte 0x02/0x06 means enabled, 0x03/0x07
/// disabled. No entry means Windows runs the Run key as it is.
pub fn startup_approved_enabled(blob: Option<&[u8]>) -> bool {
    !matches!(blob.and_then(|b| b.first()), Some(0x03) | Some(0x07))
}

/// A launch that is the computer's start rather than the user's: the Run key and the
/// LaunchAgent pass `--autostart`; a login item (SMAppService) passes nothing, so a
/// start within three minutes of boot counts too. The caller then gives Ollama's own
/// startup entry more time and does not pop permission prompts over the desktop.
pub fn is_login_launch(args: &[String], uptime: Duration) -> bool {
    args.iter().any(|a| a == "--autostart") || (uptime > Duration::ZERO && uptime < Duration::from_secs(180))
}

/// Whether this launch is a login launch (see [`is_login_launch`]).
pub fn launched_at_login() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    is_login_launch(&args, uptime())
}

mod tones;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(not(any(windows, target_os = "macos")))]
pub use unsupported::*;

/// Text with HTML special characters escaped and newlines as `<br>`.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn escape_html(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '\n' => o.push_str("<br>"),
            _ => o.push(c),
        }
    }
    o
}

/// Phone numbers, amounts and dates: a field value not worth sending as context.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(crate) fn numeric_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || " .,-+()/".contains(c))
}

/// "slack.exe" → "Slack", "com.apple.terminal" → "Terminal".
pub fn display_app_name(app: &str) -> String {
    let base = app.trim_end_matches(".exe");
    let base = if app.ends_with(".exe") { base } else { base.rsplit('.').next().unwrap_or(base) };
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "this app".into(),
    }
}

/// Classify an app for the cleanup prompt (tone hints only).
pub fn app_category(app: &str, title: &str) -> &'static str {
    let a = app.to_ascii_lowercase();
    let t = title.to_ascii_lowercase();
    if ["outlook", "thunderbird", "mail"].iter().any(|k| a.contains(k)) || t.contains("gmail") || t.contains("outlook") {
        "email"
    } else if ["slack", "teams", "discord", "telegram", "whatsapp", "signal", "messages", "mobilesms"].iter().any(|k| a.contains(k))
        || t.contains("slack")
    {
        "chat"
    } else if ["windowsterminal", "conhost", "openconsole", "powershell", "pwsh", "cmd.exe", "wezterm", "alacritty", "terminal", "iterm", "ghostty", "kitty", "warp"]
        .iter()
        .any(|k| a.contains(k))
    {
        // Before "code": iTerm's bundle id is com.googlecode.iterm2.
        "terminal"
    } else if ["code", "cursor", "windsurf", "idea", "pycharm", "rider", "devenv", "zed", "sublime", "notepad++"]
        .iter()
        .any(|k| a.contains(k))
    {
        "code"
    } else if ["chatgpt", "claude", "gemini", "perplexity"].iter().any(|k| a.contains(k) || t.contains(k)) {
        "ai_assistant"
    } else if ["winword", "pages", "obsidian", "notion", "onenote"].iter().any(|k| a.contains(k)) {
        "document"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(app_category("slack.exe", ""), "chat");
        assert_eq!(app_category("chrome.exe", "Inbox - Gmail"), "email");
        assert_eq!(app_category("Code.exe", "main.rs"), "code");
        assert_eq!(app_category("WindowsTerminal.exe", ""), "terminal");
        assert_eq!(app_category("notepad.exe", ""), "other");
        assert_eq!(app_category("com.tinyspeck.slackmacgap", ""), "chat");
        assert_eq!(app_category("com.apple.mobilesms", ""), "chat");
        assert_eq!(app_category("com.microsoft.vscode", ""), "code");
        assert_eq!(app_category("com.apple.terminal", ""), "terminal");
        assert_eq!(app_category("com.googlecode.iterm2", ""), "terminal");
        assert_eq!(app_category("com.apple.mail", ""), "email");
    }

    #[test]
    fn display_names() {
        assert_eq!(display_app_name("slack.exe"), "Slack");
        assert_eq!(display_app_name("com.apple.terminal"), "Terminal");
        assert_eq!(display_app_name(""), "this app");
        let fg = ForegroundInfo { app: "com.tinyspeck.slackmacgap".into(), name: "Slack".into(), ..Default::default() };
        assert_eq!(fg.display_name(), "Slack");
    }

    #[test]
    fn numeric() {
        assert!(numeric_only("+43 660 123"));
        assert!(!numeric_only("Hello 1"));
    }

    #[test]
    fn html_escaping() {
        assert_eq!(escape_html("a < b & c\nd"), "a &lt; b &amp; c<br>d");
    }

    #[test]
    fn chords() {
        assert_eq!(PasteChord::parse(Some("Ctrl+Shift+V")), PasteChord::CtrlShiftV);
        assert_eq!(PasteChord::parse(None), PasteChord::CtrlV);
    }

    #[test]
    fn startup_approved_blob_says_whether_windows_still_runs_the_entry() {
        // No StartupApproved entry: Windows runs whatever is in the Run key.
        assert!(startup_approved_enabled(None));
        // Task Manager → Startup apps: 0x02/0x06 enabled, 0x03/0x07 disabled (first byte of a 12-byte blob).
        assert!(startup_approved_enabled(Some(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(startup_approved_enabled(Some(&[0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(!startup_approved_enabled(Some(&[0x03, 0, 0, 0, 0xaa, 0xbb, 0, 0, 0, 0, 0, 0])));
        assert!(!startup_approved_enabled(Some(&[0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])));
        assert!(startup_approved_enabled(Some(&[])), "an empty blob is not a disable");
    }

    #[test]
    fn app_location_tells_applications_from_translocation_and_downloads() {
        use std::path::Path;
        let home = Path::new("/Users/nadia");
        let at = |p: &str| app_location_of(Path::new(p), Some(home));
        assert_eq!(at("/Applications/Dettino.app/Contents/MacOS/dettino"), AppLocation::Applications);
        assert_eq!(at("/Users/nadia/Applications/Dettino.app/Contents/MacOS/dettino"), AppLocation::Applications);
        assert_eq!(
            at("/private/var/folders/x1/T/AppTranslocation/0B3F-4C2A/d/Dettino.app/Contents/MacOS/dettino"),
            AppLocation::Translocated
        );
        assert_eq!(at("/Users/nadia/Downloads/Dettino.app/Contents/MacOS/dettino"), AppLocation::Elsewhere);
        assert_eq!(at("/Volumes/Dettino/Dettino.app/Contents/MacOS/dettino"), AppLocation::Elsewhere);
        // Not a bundle at all (cargo run, tauri dev): nothing to complain about.
        assert_eq!(at("/Users/nadia/code/dettino/target/debug/dettino"), AppLocation::Applications);
        assert_eq!(app_location_of(Path::new("/Users/x/Applications/Dettino.app/Contents/MacOS/dettino"), None), AppLocation::Elsewhere);
    }

    #[test]
    fn a_login_launch_is_the_flag_or_a_fresh_boot() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let min = |m: u64| Duration::from_secs(m * 60);
        assert!(is_login_launch(&args(&["--autostart"]), min(60)), "the Run key / LaunchAgent flag");
        assert!(is_login_launch(&args(&[]), Duration::from_secs(90)), "right after a boot, whoever started it");
        assert!(!is_login_launch(&args(&[]), min(60)));
        assert!(!is_login_launch(&args(&["--uninstall-cleanup"]), min(60)));
        assert!(!is_login_launch(&args(&[]), Duration::ZERO), "unknown uptime is reported as zero: not a boot");
    }
}
