use std::path::PathBuf;

/// Matches the bundle identifier, so the installer's "delete app data" option finds it
/// and the per-user install folder (%LOCALAPPDATA%\Dettino) never mixes with user data.
pub const APP_DIR: &str = "dev.dettino.desktop";
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    /// `%LOCALAPPDATA%\dev.dettino.desktop` on Windows, `~/Library/Application Support/dev.dettino.desktop` on macOS.
    pub fn default_root() -> Self {
        // Development / testing: run an isolated instance with its own data.
        if let Some(dir) = std::env::var_os("DETTINO_DATA_DIR").filter(|d| !d.is_empty()) {
            return Self { root: PathBuf::from(dir) };
        }
        let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        Self { root: base.join(APP_DIR) }
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("history.db")
    }
    pub fn audio(&self) -> PathBuf {
        self.root.join("audio")
    }
    pub fn sessions(&self) -> PathBuf {
        self.root.join("sessions")
    }
    pub fn models(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.root.clone(), self.audio(), self.sessions(), self.models(), self.logs()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}
