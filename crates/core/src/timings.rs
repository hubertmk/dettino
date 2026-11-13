//! Per-dictation timing breakdown, stored with each History row.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Timings {
    /// Milliseconds the chord was held (or hands-free duration).
    pub hold_ms: u64,
    pub preroll_used_ms: u64,
    pub audio_ms: u64,
    pub speech_ms: u64,
    pub asr_provider: Option<String>,
    pub asr_ms: Option<u64>,
    pub asr_hedged: bool,
    pub asr_attempts: Vec<String>,
    pub context_ms: Option<u64>,
    pub context_timed_out: bool,
    pub cleanup_provider: Option<String>,
    pub cleanup_model: Option<String>,
    pub cleanup_ms: Option<u64>,
    pub cleanup_skipped: Option<String>,
    pub cleanup_rejected: Option<String>,
    pub insert_ms: Option<u64>,
    pub insert_verify: Option<String>,
    pub clipboard_restored: Option<bool>,
    /// Key-up (or stop) to text visible.
    pub total_ms: Option<u64>,
}
