//! Global hotkeys with tap pass-through.
//!
//! The platform hook runs on its own thread and calls the pure [`ChordTracker`]
//! for every key event. Only configured chord keys are acted on; everything else
//! is passed straight through and never stored or logged.

pub mod mac_keys;
pub mod tracker;

pub use tracker::{ChordKind, ChordTracker, Decision, DiagOutcome, HookEvent, KeyDiag, KeyEvent};

/// Marker placed in `dwExtraInfo` / `kCGEventSourceUserData` of every event we inject,
/// so the hook can ignore its own replays and pastes.
pub const INJECT_MARKER: usize = 0x5746_4C57;

#[cfg(windows)]
mod windows_impl;
#[cfg(windows)]
pub use windows_impl::*;

#[cfg(target_os = "macos")]
mod macos_impl;
#[cfg(target_os = "macos")]
pub use macos_impl::*;

#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(not(any(windows, target_os = "macos")))]
pub use unsupported::*;
