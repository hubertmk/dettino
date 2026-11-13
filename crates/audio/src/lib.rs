//! Audio capture: warm microphone with pre-roll, 16 kHz resampling, level meter,
//! VAD and crash-safe WAV persistence.

pub mod engine;
pub mod level;
pub mod resample;
pub mod vad;
pub mod wav;

pub use engine::{list_devices, AudioConfig, AudioEngine, AudioEvent, DeviceInfo, DeviceKind, Take, RATE};
pub use vad::{EnergyVad, Vad};
