//! Platforms without an installer arm yet. macOS later: `https://ollama.com/download/Ollama.dmg`
//! (or `brew install ollama`), `/Applications/Ollama.app`, `open -a Ollama`.

use std::path::{Path, PathBuf};

pub const SUPPORTED: bool = false;
pub const SETUP_URL: &str = "https://ollama.com/download";
pub const SETUP_FILE: &str = "OllamaSetup";
pub const START_HINT: &str = "Start Ollama and try again.";

pub fn install_dir() -> Option<PathBuf> {
    None
}

pub fn installed() -> bool {
    false
}

pub fn launch() -> Result<(), String> {
    Err("Starting Ollama is not implemented on this platform yet.".into())
}

pub async fn run_installer(_exe: &Path) -> Result<(), String> {
    Err("Installing Ollama is not implemented on this platform yet.".into())
}
