//! Windows: Ollama ships as a per-user Inno Setup installer (no admin rights);
//! the tray app `ollama app.exe` runs the server.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

pub const SUPPORTED: bool = true;
pub const SETUP_URL: &str = "https://ollama.com/download/OllamaSetup.exe";
pub const SETUP_FILE: &str = "OllamaSetup.exe";
pub const START_HINT: &str = "Start Ollama from the Start menu and try again.";

/// Inno Setup's per-user uninstall entry for Ollama.
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{44E83376-CE68-45EB-8FC1-393500EB558C}_is1";
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Standard Inno Setup switches for an unattended per-user install.
const SILENT_ARGS: [&str; 4] = ["/SP-", "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"];

pub fn install_dir() -> Option<PathBuf> {
    if let Some(loc) = app_platform::hkcu_string(UNINSTALL_KEY, "InstallLocation") {
        let p = PathBuf::from(loc.trim_end_matches('\\'));
        if p.is_dir() {
            return Some(p);
        }
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    let p = PathBuf::from(local).join("Programs").join("Ollama");
    p.is_dir().then_some(p)
}

fn app_exe() -> Option<PathBuf> {
    install_dir().map(|d| d.join("ollama app.exe")).filter(|p| p.is_file())
}

fn cli_exe() -> Option<PathBuf> {
    install_dir().map(|d| d.join("ollama.exe")).filter(|p| p.is_file())
}

pub fn installed() -> bool {
    app_exe().is_some() || cli_exe().is_some()
}

/// Start the tray app (which runs the server), or `ollama serve` if only the CLI exists.
pub fn launch() -> Result<(), String> {
    let mut cmd = if let Some(app) = app_exe() {
        std::process::Command::new(app)
    } else if let Some(cli) = cli_exe() {
        let mut c = std::process::Command::new(cli);
        c.arg("serve");
        c
    } else {
        return Err("Ollama is not installed.".into());
    };
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't start Ollama: {e}"))
}

/// Verify the installer's Authenticode signature and publisher, then run it silently.
pub async fn run_installer(exe: &Path) -> Result<(), String> {
    let path = exe.to_path_buf();
    let signer = tokio::task::spawn_blocking(move || app_platform::verify_authenticode(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("The downloaded installer could not be verified ({e}). It was not run."))?;
    if !signer.to_ascii_lowercase().contains("ollama") {
        return Err(format!("The downloaded installer is signed by \"{signer}\", not by Ollama. It was not run."));
    }
    let status = tokio::process::Command::new(exe)
        .args(SILENT_ARGS)
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| format!("Couldn't run the Ollama installer: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("The Ollama installer stopped with code {}.", status.code().unwrap_or(-1)))
    }
}

/// Run Ollama's own uninstaller silently, then delete `~/.ollama`: in silent mode the
/// uninstaller keeps the downloaded models. It stops the Ollama processes itself.
pub fn uninstall() -> Result<(), String> {
    let cmd = app_platform::hkcu_string(UNINSTALL_KEY, "UninstallString").ok_or("Ollama has no uninstall entry.")?;
    let exe = cmd.trim().trim_matches('"').to_string();
    std::process::Command::new(&exe)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("Couldn't run {exe}: {e}"))?;
    // Inno Setup's uninstaller hands over to a copy of itself in %TEMP% and returns early.
    let t0 = std::time::Instant::now();
    while installed() && t0.elapsed() < std::time::Duration::from_secs(60) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if installed() {
        return Err("Ollama's uninstaller did not finish within a minute.".into());
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        let _ = std::fs::remove_dir_all(PathBuf::from(home).join(".ollama"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a machine with Ollama's per-user installer entry, detection must agree with it.
    #[test]
    fn registry_entry_implies_installed() {
        if let Some(loc) = app_platform::hkcu_string(UNINSTALL_KEY, "InstallLocation") {
            assert!(install_dir().is_some(), "InstallLocation {loc} should resolve");
            assert!(installed());
        }
    }

    #[test]
    fn silent_flags_are_inno_setup_switches() {
        assert!(SILENT_ARGS.contains(&"/VERYSILENT"));
        assert!(SILENT_ARGS.iter().all(|a| a.starts_with('/')));
    }
}
