//! Placeholder for platforms without an implementation yet (Linux): no screen
//! context, no paste, no foreground detection.

use crate::*;

pub fn foreground() -> Option<ForegroundInfo> {
    None
}
pub fn capture_context(_req: ContextRequest, _budget: Duration) -> ContextSnapshot {
    ContextSnapshot::default()
}
pub fn read_focused_text(_budget: Duration) -> Option<String> {
    None
}
pub fn select_recent_text(_text: &str, _budget: Duration) -> bool {
    false
}
pub fn paste(_req: &PasteRequest) -> PasteOutcome {
    PasteOutcome::Failed("Pasting is not implemented on this platform yet".into())
}
pub fn read_clipboard_text() -> Option<String> {
    None
}
pub fn set_clipboard_text(_text: &str) -> bool {
    false
}
pub fn copy_selection() -> Option<String> {
    None
}
pub fn self_is_elevated() -> bool {
    false
}
pub fn raise_topmost(_hwnd: isize) {}
pub fn alert(title: &str, text: &str) {
    tracing::warn!(title, text, "no message box on this platform");
}
pub fn play_sound(_kind: SoundKind, _style: app_core::SoundStyle, _volume: f32) {}
pub fn gpus() -> Vec<GpuInfo> {
    Vec::new()
}
/// Physical RAM in MB; 0 when unknown.
pub fn total_ram_mb() -> u64 {
    0
}
pub fn free_disk_mb(_path: &std::path::Path) -> Option<u64> {
    None
}
/// Read a string value under HKEY_CURRENT_USER (Windows only).
pub fn hkcu_string(_subkey: &str, _value: &str) -> Option<String> {
    None
}
pub fn set_launch_at_login(_enabled: bool) -> Result<(), String> {
    Err("not implemented on this platform yet".into())
}
/// Whether the OS will really start Dettino at login (`Ok(false)`: not registered or switched off).
pub fn launch_at_login_active() -> Result<bool, String> {
    Err("not implemented on this platform yet".into())
}
/// Time since boot; zero when unknown.
pub fn uptime() -> Duration {
    Duration::ZERO
}

pub fn accent_color() -> Option<String> {
    None
}

pub fn taskbar_is_light() -> bool {
    false
}
