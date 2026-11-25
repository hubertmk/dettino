//! Local model catalog and download manager: resumable downloads with progress,
//! size check against the size the server announces, and extraction of `.tar.bz2` archives from the sherpa-onnx releases.

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
    /// Download size shown before the server answers; the server's own size is what is checked.
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
        m("parakeet-tdt-0.6b-v2-int8", "parakeet", "Parakeet TDT 0.6B v2", "Fastest and most accurate for English. Runs on any modern CPU.", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2", 482_440_461, "CC-BY-4.0 (NVIDIA)", "parakeet", true, true, true),
        m("parakeet-tdt-0.6b-v3-int8", "parakeet", "Parakeet TDT 0.6B v3", "Like v2, adds 25 European languages (for a later version).", "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2", 487_211_443, "CC-BY-4.0 (NVIDIA)", "parakeet-v3", true, false, false),
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

/// Parse a `Content-Range` header into `(first byte, total size)`.
/// `bytes 100-499/500` gives `(Some(100), Some(500))`; `bytes */500` (sent with
/// HTTP 416) gives `(None, Some(500))`; an unknown total `*` gives `None`.
fn parse_content_range(value: &str) -> (Option<u64>, Option<u64>) {
    let Some(rest) = value.trim().strip_prefix("bytes ") else { return (None, None) };
    let Some((range, total)) = rest.split_once('/') else { return (None, None) };
    let start = range.split_once('-').and_then(|(s, _)| s.trim().parse().ok());
    (start, total.trim().parse().ok())
}

/// Download `url` into `part`, resuming a partial file with a Range request.
/// Returns the final size on disk.
///
/// The size the server announces (`Content-Length`, or the total in
/// `Content-Range` when resuming) is the ground truth for completeness: a file
/// that ends up any other size is deleted. `size_hint` only seeds the progress
/// bar until the server has answered; `0` means unknown.
///
/// A `.part` file that the server will not continue (HTTP 416 for a file that
/// is not exactly the server's size, or a `Content-Range` that does not start
/// where the file ends) is deleted and the download starts over once, instead
/// of failing forever.
pub async fn download_file(
    url: &str,
    part: &Path,
    size_hint: u64,
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
        let restart = |why: String| async move {
            let _ = tokio::fs::remove_file(part).await;
            if restarted {
                Err(format!("Download failed: {why}"))
            } else {
                tracing::warn!("{why}; starting the download over");
                Ok(())
            }
        };
        let mut req = client.get(url);
        if existing > 0 {
            req = req.header("Range", format!("bytes={existing}-"));
        }
        let resp = req.send().await.map_err(|e| format!("Download failed: {e}"))?;
        let status = resp.status();
        let content_range = resp.headers().get("content-range").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        if status.as_u16() == 416 {
            // Complete from an earlier run that stopped before unpacking: the
            // server has nothing past the end of our file, and confirms its size.
            if let (_, Some(total)) = parse_content_range(&content_range) {
                if total == existing {
                    on_progress(Progress::new(id, existing, existing, "downloading"));
                    return Ok(existing);
                }
            }
            restart(format!("the server has nothing to add to the partial file ({existing} bytes, HTTP 416 {content_range:?})")).await?;
            restarted = true;
            continue;
        }
        if !status.is_success() {
            return Err(format!("Download failed: HTTP {}", status.as_u16()));
        }
        let resumed = status.as_u16() == 206;
        // 0 when the server did not say how big the file is.
        let expected = if resumed {
            let (start, total) = parse_content_range(&content_range);
            if start != Some(existing) {
                restart(format!("the server resumed at a different offset ({content_range:?})")).await?;
                restarted = true;
                continue;
            }
            total.unwrap_or(0)
        } else {
            resp.content_length().unwrap_or(0)
        };
        let mut done = if resumed { existing } else { 0 };
        let total = if expected > 0 { expected } else { size_hint };
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
        if expected > 0 && done != expected {
            let _ = tokio::fs::remove_file(part).await;
            return Err(format!("Downloaded size {done} does not match the {expected} bytes the server announced; the download was reset, try again."));
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
    fn content_range_parses_resume_and_416_forms() {
        assert_eq!(parse_content_range("bytes 100-499/500"), (Some(100), Some(500)));
        assert_eq!(parse_content_range("bytes */500"), (None, Some(500)));
        assert_eq!(parse_content_range("bytes 100-499/*"), (Some(100), None));
        assert_eq!(parse_content_range(""), (None, None));
        assert_eq!(parse_content_range("items 1-2/3"), (None, None));
    }

    /// A minimal HTTP/1.1 file server with Range support, so the download code
    /// is exercised against real 200 / 206 / 416 answers.
    async fn serve(body: &'static [u8]) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let mut head = Vec::new();
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        let n = sock.read(&mut buf).await.unwrap();
                        if n == 0 { return; }
                        head.extend_from_slice(&buf[..n]);
                    }
                    let head = String::from_utf8_lossy(&head).to_string();
                    let start = head.lines().find_map(|l| l.strip_prefix("Range: bytes=")).and_then(|r| r.trim_end_matches('-').parse::<u64>().ok());
                    let len = body.len() as u64;
                    let mut out = Vec::new();
                    match start {
                        Some(s) if s >= len => {
                            out.extend_from_slice(format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
                        }
                        Some(s) => {
                            out.extend_from_slice(format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {s}-{}/{len}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", len - 1, len - s).as_bytes());
                            out.extend_from_slice(&body[s as usize..]);
                        }
                        None => {
                            out.extend_from_slice(format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n").as_bytes());
                            out.extend_from_slice(body);
                        }
                    }
                    sock.write_all(&out).await.unwrap();
                    sock.shutdown().await.ok();
                });
            }
        });
        format!("http://{addr}/file.bin")
    }

    const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dettino-models-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("file.bin.part")
    }

    #[tokio::test]
    async fn a_stale_catalog_size_does_not_reject_a_complete_download() {
        let url = serve(BODY).await;
        let part = scratch("stale");
        // The catalog claims a different size than the server serves (the upstream
        // archive was re-uploaded). The server's size wins.
        let n = download_file(&url, &part, BODY.len() as u64 + 27_924, "t", &|_| {}).await.unwrap();
        assert_eq!(n, BODY.len() as u64);
        assert_eq!(std::fs::read(&part).unwrap(), BODY);
    }

    #[tokio::test]
    async fn the_progress_total_comes_from_the_server() {
        let url = serve(BODY).await;
        let part = scratch("progress");
        let totals = std::sync::Mutex::new(Vec::new());
        download_file(&url, &part, 999, "t", &|p| totals.lock().unwrap().push(p.total)).await.unwrap();
        assert!(totals.lock().unwrap().iter().all(|&t| t == BODY.len() as u64), "{:?}", totals.lock().unwrap());
    }

    #[tokio::test]
    async fn resumes_a_partial_file() {
        let url = serve(BODY).await;
        let part = scratch("resume");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, &BODY[..10]).unwrap();
        let n = download_file(&url, &part, 0, "t", &|_| {}).await.unwrap();
        assert_eq!(n, BODY.len() as u64);
        assert_eq!(std::fs::read(&part).unwrap(), BODY);
    }

    #[tokio::test]
    async fn keeps_a_complete_file_from_an_earlier_run() {
        let url = serve(BODY).await;
        let part = scratch("complete");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, BODY).unwrap();
        let n = download_file(&url, &part, 0, "t", &|_| {}).await.unwrap();
        assert_eq!(n, BODY.len() as u64);
        assert_eq!(std::fs::read(&part).unwrap(), BODY);
    }

    #[tokio::test]
    async fn starts_over_when_the_partial_file_is_larger_than_the_download() {
        let url = serve(BODY).await;
        let part = scratch("oversized");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        let mut big = BODY.to_vec();
        big.extend_from_slice(b"stale tail from an older upload");
        std::fs::write(&part, &big).unwrap();
        let n = download_file(&url, &part, 0, "t", &|_| {}).await.unwrap();
        assert_eq!(n, BODY.len() as u64);
        assert_eq!(std::fs::read(&part).unwrap(), BODY);
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
