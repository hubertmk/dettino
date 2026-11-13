//! macOS: Ollama ships as `Ollama.app` in a zip. Unpacking it into /Applications
//! needs no admin rights for an admin user; otherwise it goes to ~/Applications.
//! The app runs the server; the bundled CLI is the fallback.

use std::path::{Path, PathBuf};

pub const SUPPORTED: bool = true;
pub const SETUP_URL: &str = "https://ollama.com/download/Ollama-darwin.zip";
pub const SETUP_FILE: &str = "Ollama-darwin.zip";
pub const START_HINT: &str = "Open Ollama from your Applications folder and try again.";

fn app_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

/// The installed `Ollama.app`.
pub fn install_dir() -> Option<PathBuf> {
    app_dirs().into_iter().map(|d| d.join("Ollama.app")).find(|p| p.is_dir())
}

fn cli_exe() -> Option<PathBuf> {
    let bundled = install_dir().map(|a| a.join("Contents/Resources/ollama"));
    bundled
        .into_iter()
        .chain(["/usr/local/bin/ollama", "/opt/homebrew/bin/ollama"].map(PathBuf::from))
        .find(|p| p.is_file())
}

pub fn installed() -> bool {
    install_dir().is_some() || cli_exe().is_some()
}

/// Start the app in the background (it runs the server), or `ollama serve` if only the CLI exists.
pub fn launch() -> Result<(), String> {
    let mut cmd = if let Some(app) = install_dir() {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("-g").arg("-a").arg(app);
        c
    } else if let Some(cli) = cli_exe() {
        let mut c = std::process::Command::new(cli);
        c.arg("serve");
        c
    } else {
        return Err("Ollama is not installed.".into());
    };
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't start Ollama: {e}"))
}

async fn ditto(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = tokio::process::Command::new("/usr/bin/ditto")
        .args(args)
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| format!("Couldn't unpack Ollama: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Unpacking Ollama stopped with code {}.", status.code().unwrap_or(-1)))
    }
}

/// The unpacked app must carry an intact Developer ID signature from Ollama before it
/// is copied anywhere. A download has no quarantine attribute, so Gatekeeper would
/// never look at it on its own.
async fn verify_app(app: &Path) -> Result<(), String> {
    let intact = tokio::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .status()
        .await
        .map_err(|e| format!("Couldn't check the Ollama signature: {e}"))?
        .success();
    if !intact {
        return Err("The downloaded Ollama.app has a missing or broken code signature. It was not installed.".into());
    }
    let out = tokio::process::Command::new("/usr/bin/codesign")
        .args(["-dvv"])
        .arg(app)
        .output()
        .await
        .map_err(|e| format!("Couldn't read the Ollama signature: {e}"))?;
    // codesign prints the signing chain on stderr.
    let info = String::from_utf8_lossy(&out.stderr);
    let by_ollama = info
        .lines()
        .map(str::trim)
        .any(|l| l.starts_with("Authority=Developer ID Application:") && l.to_ascii_lowercase().contains("ollama"));
    if !by_ollama {
        return Err("The downloaded Ollama.app is not signed with Ollama's Developer ID. It was not installed.".into());
    }
    Ok(())
}

/// Unpack the zip next to it, verify `Ollama.app`, then copy it into the first
/// writable Applications folder.
pub async fn run_installer(zip: &Path) -> Result<(), String> {
    let staging = zip.with_extension("unpacked");
    let _ = tokio::fs::remove_dir_all(&staging).await;
    ditto(&["-x".as_ref(), "-k".as_ref(), zip.as_os_str(), staging.as_os_str()]).await?;
    let app = staging.join("Ollama.app");
    if !app.is_dir() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err("The Ollama download did not contain Ollama.app.".into());
    }
    if let Err(e) = verify_app(&app).await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(e);
    }
    let mut last_err = String::from("No Applications folder is writable.");
    for dir in app_dirs() {
        if tokio::fs::create_dir_all(&dir).await.is_err() {
            continue;
        }
        let dest = dir.join("Ollama.app");
        match ditto(&[app.as_os_str(), dest.as_os_str()]).await {
            Ok(()) => {
                let _ = tokio::fs::remove_dir_all(&staging).await;
                return Ok(());
            }
            Err(e) => last_err = e,
        }
    }
    let _ = tokio::fs::remove_dir_all(&staging).await;
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_consistent() {
        if install_dir().is_some() {
            assert!(installed());
        }
        assert!(app_dirs()[0].ends_with("Applications"));
    }
}
