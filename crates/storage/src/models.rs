//! Local model catalog and download manager: resumable downloads with progress,
//! size check and extraction of `.tar.bz2` archives from the sherpa-onnx releases.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub family: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub bytes: u64,
    pub license: String,
    /// Folder under `models/` the archive is extracted into.
    pub dir: String,
    pub archive: bool,
    pub english_only: bool,
    pub recommended: bool,
}

pub fn catalog() -> Vec<ModelInfo> {
    let m = |id: &str, family: &str, name: &str, desc: &str, file: &str, bytes: u64, license: &str, dir: &str, archive: bool, en: bool, rec: bool| ModelInfo {
        id: id.into(),
        family: family.into(),
        name: name.into(),
        description: desc.into(),
        url: format!("{BASE}/{file}"),
        bytes,
        license: license.into(),
        dir: dir.into(),
        archive,
        english_only: en,
        recommended: rec,
    };
    vec![
        m("silero-vad", "vad", "Silero voice detection", "Detects when you speak, so silence is never transcribed.", "silero_vad.onnx", 643_854, "MIT", "vad", false, false, true),
        m("parakeet-tdt-0.6b-v2-int8", "parakeet", "Parakeet TDT 0.6B v2", "Fastest and most accurate for English. Runs on any modern CPU.", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2", 482_468_385, "CC-BY-4.0 (NVIDIA)", "parakeet", true, true, true),
        m("parakeet-tdt-0.6b-v3-int8", "parakeet", "Parakeet TDT 0.6B v3", "Like v2, adds 25 European languages (for a later version).", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2", 487_170_055, "CC-BY-4.0 (NVIDIA)", "parakeet-v3", true, false, false),
        m("whisper-turbo", "whisper", "Whisper Large v3 Turbo", "Robust with accents and noise; slower than Parakeet on CPU.", "sherpa-onnx-whisper-turbo.tar.bz2", 563_790_207, "MIT (OpenAI)", "whisper", true, false, false),
        m("whisper-small.en", "whisper", "Whisper Small (English)", "Smaller Whisper for older computers.", "sherpa-onnx-whisper-small.en.tar.bz2", 635_693_775, "MIT (OpenAI)", "whisper-small", true, true, false),
    ]
}

/// Whether a model is present on disk.
pub fn installed(models_dir: &Path, m: &ModelInfo) -> bool {
    let target = models_dir.join(&m.dir);
    if !m.archive {
        return target.join(m.url.rsplit('/').next().unwrap_or("")).exists();
    }
    // any .onnx inside the folder (one level deep)
    std::fs::read_dir(&target)
        .map(|rd| {
            rd.flatten().any(|e| {
                let p = e.path();
                (p.extension().map(|x| x == "onnx").unwrap_or(false))
                    || (p.is_dir()
                        && std::fs::read_dir(&p)
                            .map(|r| r.flatten().any(|f| f.path().extension().map(|x| x == "onnx").unwrap_or(false)))
                            .unwrap_or(false))
            })
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub id: String,
    pub done: u64,
    pub total: u64,
    pub stage: &'static str,
    /// Free-text status for the UI (e.g. Ollama's "pulling manifest"); `None` for plain downloads.
    #[serde(default)]
    pub detail: Option<String>,
}

impl Progress {
    pub fn new(id: &str, done: u64, total: u64, stage: &'static str) -> Self {
        Self { id: id.into(), done, total, stage, detail: None }
    }
}

/// Download `url` into `part`, resuming a partial file with a Range request.
/// Returns the final size on disk. `expected_bytes == 0` skips the size check.
///
/// A `.part` file that the server will not continue (HTTP 416, a `Content-Range`
/// that does not start where the file ends) or that ends up the wrong size is
/// deleted and the download starts over once, instead of failing forever.
pub async fn download_file(
    url: &str,
    part: &Path,
    expected_bytes: u64,
    id: &str,
    on_progress: &(impl Fn(Progress) + Send + Sync),
) -> Result<u64, String> {
    if let Some(dir) = part.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let mut restarted = false;
    loop {
        let existing = tokio::fs::metadata(part).await.map(|md| md.len()).unwrap_or(0);
        if expected_bytes > 0 && existing == expected_bytes {
            // Complete from an earlier run that stopped before unpacking.
            on_progress(Progress::new(id, existing, existing, "downloading"));
            return Ok(existing);
        }
        let restart = |why: String| async move {
            let _ = tokio::fs::remove_file(part).await;
            if restarted {
                Err(format!("Download failed: {why}"))
            } else {
                tracing::warn!("{why}; starting the download over");
                Ok(())
            }
        };
        if expected_bytes > 0 && existing > expected_bytes {
            restart(format!("the partial file is larger ({existing} bytes) than the download ({expected_bytes})")).await?;
            restarted = true;
            continue;
        }
        let mut req = client.get(url);
        if existing > 0 {
            req = req.header("Range", format!("bytes={existing}-"));
        }
        let resp = req.send().await.map_err(|e| format!("Download failed: {e}"))?;
        let status = resp.status();
        if status.as_u16() == 416 {
            restart("the server has nothing to add to the partial file (HTTP 416)".into()).await?;
            restarted = true;
            continue;
        }
        if !status.is_success() {
            return Err(format!("Download failed: HTTP {}", status.as_u16()));
        }
        let resumed = status.as_u16() == 206;
        if resumed {
            let range = resp.headers().get("content-range").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            if !range.starts_with(&format!("bytes {existing}-")) {
                restart(format!("the server resumed at a different offset ({range:?})")).await?;
                restarted = true;
                continue;
            }
        }
        let mut done = if resumed { existing } else { 0 };
        let total = if resumed { existing + resp.content_length().unwrap_or(0) } else { resp.content_length().unwrap_or(expected_bytes) };
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(part)
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = resp.bytes_stream();
        let mut last_report = 0u64;
        on_progress(Progress::new(id, done, total, "downloading"));
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download interrupted: {e}. It will resume where it stopped."))?;
            f.write_all(&chunk).await.map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            if done - last_report > 2_000_000 || done == total {
                last_report = done;
                on_progress(Progress::new(id, done, total, "downloading"));
            }
        }
        f.flush().await.map_err(|e| e.to_string())?;
        drop(f);
        if expected_bytes > 0 && done != expected_bytes {
            let _ = tokio::fs::remove_file(part).await;
            return Err(format!("Downloaded size {done} does not match the expected {expected_bytes}; the download was reset, try again."));
        }
        return Ok(done);
    }
}

/// Download (resuming a `.part` file) and install a model.
pub async fn download(
    models_dir: &Path,
    m: &ModelInfo,
    on_progress: impl Fn(Progress) + Send + Sync,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(models_dir).await.map_err(|e| e.to_string())?;
    let file_name = m.url.rsplit('/').next().unwrap_or("model.bin").to_string();
    let part = models_dir.join(format!("{file_name}.part"));
    let done = download_file(&m.url, &part, m.bytes, &m.id, &on_progress).await?;
    let total = done;

    let target = models_dir.join(&m.dir);
    if m.archive {
        on_progress(Progress::new(&m.id, done, total, "installing"));
        // Unpack next to the target and swap it in when complete: a half-extracted
        // folder must never count as an installed model.
        let tmp = models_dir.join(format!(".tmp-{}", m.id));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let part2 = part.clone();
        let tmp2 = tmp.clone();
        let unpacked = tokio::task::spawn_blocking(move || -> Result<(), String> {
            std::fs::create_dir_all(&tmp2).map_err(|e| e.to_string())?;
            let file = std::fs::File::open(&part2).map_err(|e| e.to_string())?;
            let dec = bzip2::read::BzDecoder::new(std::io::BufReader::new(file));
            tar::Archive::new(dec).unpack(&tmp2).map_err(|e| format!("Unpacking failed: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        if let Err(e) = unpacked {
            // A damaged archive must not be resumed from: start over next time.
            let _ = tokio::fs::remove_dir_all(&tmp).await;
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }
        let _ = tokio::fs::remove_dir_all(&target).await;
        tokio::fs::rename(&tmp, &target).await.map_err(|e| e.to_string())?;
        let _ = tokio::fs::remove_file(&part).await;
    } else {
        tokio::fs::create_dir_all(&target).await.map_err(|e| e.to_string())?;
        tokio::fs::rename(&part, target.join(&file_name)).await.map_err(|e| e.to_string())?;
    }
    on_progress(Progress::new(&m.id, done, total, "ready"));
    Ok(target)
}

pub fn remove(models_dir: &Path, m: &ModelInfo) -> std::io::Result<()> {
    let target = models_dir.join(&m.dir);
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sane() {
        let c = catalog();
        assert!(c.iter().any(|m| m.family == "vad"));
        assert!(c.iter().filter(|m| m.recommended && m.family == "parakeet").count() == 1);
        let ids: std::collections::HashSet<_> = c.iter().map(|m| &m.id).collect();
        assert_eq!(ids.len(), c.len());
    }

    #[test]
    fn installed_detects_existing_models() {
        let Some(base) = dirs::data_local_dir() else { return };
        let models = base.join("dev.dettino.desktop").join("models");
        if !models.exists() {
            return;
        }
        let c = catalog();
        let vad = c.iter().find(|m| m.id == "silero-vad").unwrap();
        if models.join("vad/silero_vad.onnx").exists() {
            assert!(installed(&models, vad));
        }
    }
}
