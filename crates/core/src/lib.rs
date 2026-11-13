//! Core of the dictation app: settings model, capture state machine, provider
//! traits, the fallback chain runner and per-dictation timings.
//!
//! This crate has no OS dependencies so every piece of logic here is unit-testable.

pub mod chain;
pub mod config;
pub mod keys;
pub mod machine;
pub mod provider;
pub mod stats;
pub mod timings;

pub use chain::{run_chain, ChainAttempt, ChainOutcome};
pub use config::*;
pub use keys::{ChordSpec, Hotkeys};
pub use machine::{CaptureMachine, CaptureState, Input, Mode, Output};
pub use provider::*;
pub use stats::ProviderStats;
pub use timings::Timings;
