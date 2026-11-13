//! Local storage: paths, settings file, OS-keychain secrets, encrypted history,
//! provider statistics and the model download manager.
//!
//! Deviation from the plan (risk R14 fallback): history is encrypted at the
//! record level with XChaCha20-Poly1305 on top of plain SQLite instead of
//! SQLCipher, because SQLCipher's vendored OpenSSL needs nmake and a Visual Studio
//! developer shell to build. The key lives in the OS keychain either way.

pub mod history;
pub mod models;
pub mod paths;
pub mod secrets;
pub mod settings;

pub use history::{History, HistoryRecord, Status};
pub use paths::Paths;
