//! Placeholder backend for platforms without an implementation (Linux).
//! It compiles and reports a clear error instead of failing silently.

use crate::tracker::{ChordKind, HookEvent, KeyEvent};
use app_core::keys::Hotkeys;
use crossbeam_channel::Receiver;
use std::time::Instant;

pub fn start(_hotkeys: &Hotkeys) -> Result<Receiver<(HookEvent, Instant)>, String> {
    Err("Global shortcuts are not implemented on this platform yet".into())
}
pub fn is_active() -> bool {
    false
}
pub fn stop() {}
pub fn set_hotkeys(_h: &Hotkeys) {}
pub fn set_swallow_space(_on: bool) {}
pub fn set_swallow_esc(_on: bool) {}
pub fn set_trace(_on: bool) {}
pub fn user_key_counter() -> u64 {
    0
}
pub fn replay(_kind: ChordKind) {}
pub fn resync() -> Vec<HookEvent> {
    Vec::new()
}
pub fn physical_mods() -> u8 {
    0
}
pub fn send_keys(_seq: &[KeyEvent]) -> bool {
    false
}
pub fn send_chord(_mods: u8, _key: u16) -> bool {
    false
}
pub fn send_unicode(_text: &str) -> bool {
    false
}
