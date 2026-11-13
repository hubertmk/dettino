//! On-device transcription through sherpa-onnx: NVIDIA Parakeet (NeMo
//! transducer) and Whisper ONNX exports, plus the Silero VAD.
//!
//! Deviation from the plan: whisper.cpp/whisper-rs was replaced by sherpa-onnx's
//! Whisper support so that one prebuilt runtime covers every local model and no
//! CMake or CUDA toolkit is needed to build.

use app_core::{AsrProvider, AsrRequest, ProviderError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod silero;

/// Find the first file in `dir` (recursively, one level) whose name starts with
/// `prefix` and ends with `suffix`, preferring int8 variants.
fn find(dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    let mut cands = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                dirs.push(e.path());
            }
        }
    }
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with(prefix) && name.ends_with(suffix) {
                    cands.push(e.path());
                }
            }
        }
    }
    cands.sort_by_key(|p| if p.to_string_lossy().contains("int8") { 0 } else { 1 });
    cands.into_iter().next()
}

fn threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8) as i32
}

fn to_f32(pcm: &[i16]) -> Vec<f32> {
    pcm.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// 10 ms at 16 kHz.
const FRAME: usize = 160;
/// Noise floor for gated gaps: RMS of 4 LSB, about -78 dBFS.
const GAP_NOISE_RMS: f32 = 4.0 / 32768.0;

/// Microphones with a noise gate (some USB headsets and speakerphones) turn pauses
/// into exact digital silence. On a long take those frames sit on the log floor of
/// Parakeet's filterbank and throw off its per-feature normalisation, and the model
/// then decodes blank everywhere: an empty transcript from clear speech. Runs of
/// exact zeros (at least one frame long) get a faint noise floor instead.
///
/// Raising quiet speech to a target level was tried too: it rescued a few more long
/// takes but lost short quiet ones and changed words either way, so it is not done.
fn fill_digital_silence(samples: &mut [f32]) {
    // Uniform noise in [-a, a] has an RMS of a / sqrt(3).
    let a = GAP_NOISE_RMS * 3f32.sqrt();
    let mut seed = 0x2545_f491_u32;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * a
    };
    let mut i = 0;
    while i < samples.len() {
        if samples[i] != 0.0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < samples.len() && samples[i] == 0.0 {
            i += 1;
        }
        if i - start >= FRAME {
            samples[start..i].iter_mut().for_each(|s| *s = noise());
        }
    }
}

/// A piece of the second pass is cut 2.5–5 s after the previous cut, at the quietest
/// 100 ms in that span, and the last piece may run to 6 s.
const PIECE_MIN: usize = 16_000 * 5 / 2;
const PIECE_MAX: usize = 16_000 * 5;
const PIECE_LAST: usize = 16_000 * 6;

/// Ranges that cover `samples`, cut at quiet moments, for decoding a take piece by
/// piece. The same blank decode that fill_digital_silence avoids also happens on
/// takes without gated gaps; on shorter pieces the normalisation has less to go
/// wrong with, and each piece decodes on its own.
fn pieces(samples: &[f32]) -> Vec<std::ops::Range<usize>> {
    // Energy of every 10 ms frame, smoothed over 100 ms.
    let energy: Vec<f32> = samples.chunks(FRAME).map(|f| f.iter().map(|s| s * s).sum::<f32>()).collect();
    let smooth = |i: usize| energy[i.saturating_sub(5)..(i + 5).min(energy.len())].iter().sum::<f32>();
    let mut out = Vec::new();
    let mut start = 0;
    while samples.len() - start > PIECE_LAST {
        let (lo, hi) = ((start + PIECE_MIN) / FRAME, (start + PIECE_MAX) / FRAME);
        let quiet = (lo..hi).min_by(|&a, &b| smooth(a).total_cmp(&smooth(b))).unwrap_or(hi);
        out.push(start..quiet * FRAME);
        start = quiet * FRAME;
    }
    out.push(start..samples.len());
    out
}

/// Model files for Parakeet; `dir` is the folder the archive was extracted into.
#[derive(Debug, Clone)]
pub struct ParakeetFiles {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

impl ParakeetFiles {
    pub fn locate(dir: &Path) -> Option<Self> {
        Some(Self {
            encoder: find(dir, "encoder", ".onnx")?,
            decoder: find(dir, "decoder", ".onnx")?,
            joiner: find(dir, "joiner", ".onnx")?,
            tokens: find(dir, "tokens", ".txt")?,
        })
    }
}

pub struct ParakeetProvider {
    id: String,
    dir: PathBuf,
    model: Arc<Mutex<Option<sherpa_rs::transducer::TransducerRecognizer>>>,
}

impl ParakeetProvider {
    pub fn new(id: &str, dir: PathBuf) -> Self {
        Self { id: id.into(), dir, model: Arc::new(Mutex::new(None)) }
    }

    pub fn is_installed(&self) -> bool {
        ParakeetFiles::locate(&self.dir).is_some()
    }

    fn load(dir: &Path) -> Result<sherpa_rs::transducer::TransducerRecognizer, ProviderError> {
        let files = ParakeetFiles::locate(dir)
            .ok_or_else(|| ProviderError::ModelMissing("Parakeet".into()))?;
        let cfg = sherpa_rs::transducer::TransducerConfig {
            encoder: files.encoder.to_string_lossy().into(),
            decoder: files.decoder.to_string_lossy().into(),
            joiner: files.joiner.to_string_lossy().into(),
            tokens: files.tokens.to_string_lossy().into(),
            num_threads: threads(),
            sample_rate: 16_000,
            feature_dim: 80,
            decoding_method: "greedy_search".into(),
            model_type: "nemo_transducer".into(),
            debug: false,
            ..Default::default()
        };
        let t0 = std::time::Instant::now();
        let r = sherpa_rs::transducer::TransducerRecognizer::new(cfg)
            .map_err(|e| ProviderError::Other(format!("Parakeet failed to load: {e}")))?;
        tracing::info!(ms = t0.elapsed().as_millis() as u64, "Parakeet loaded");
        Ok(r)
    }

    pub fn unload(&self) {
        if let Ok(mut g) = self.model.lock() {
            *g = None;
        }
    }

    fn run(&self, pcm: Arc<Vec<i16>>) -> impl FnOnce() -> Result<String, ProviderError> + Send + 'static {
        let model = self.model.clone();
        let dir = self.dir.clone();
        move || {
            let mut g = model.lock().map_err(|_| ProviderError::Other("model lock poisoned".into()))?;
            if g.is_none() {
                *g = Some(Self::load(&dir)?);
            }
            let r = g.as_mut().unwrap();
            let mut samples = to_f32(&pcm);
            fill_digital_silence(&mut samples);
            let text = r.transcribe(16_000, &samples).trim().to_string();
            // Blank on the whole take: a second pass, piece by piece. It only runs
            // when the alternative is "Didn't hear anything", so it cannot change a
            // transcript that already has words. Takes up to 6 s are one piece.
            let parts = pieces(&samples);
            let n = parts.len();
            if !text.is_empty() || n < 2 {
                return Ok(text);
            }
            let text = parts
                .into_iter()
                .map(|p| r.transcribe(16_000, &samples[p]).trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            tracing::info!(pieces = n, chars = text.chars().count(), "Parakeet: blank decode, second pass in pieces");
            Ok(text)
        }
    }
}

#[async_trait]
impl AsrProvider for ParakeetProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        true
    }
    async fn warmup(&self) -> Result<(), ProviderError> {
        // Load and run one short silent inference so the first dictation is fast.
        let f = self.run(Arc::new(vec![0i16; 8000]));
        tokio::task::spawn_blocking(f).await.map_err(|e| ProviderError::Other(e.to_string()))?.map(|_| ())
    }
    async fn transcribe(&self, req: &AsrRequest) -> Result<String, ProviderError> {
        let f = self.run(req.pcm.clone());
        tokio::task::spawn_blocking(f).await.map_err(|e| ProviderError::Other(e.to_string()))?
    }
    fn timeout(&self, audio_ms: u64) -> Duration {
        // generous on first use (model load); steady state is ~0.1× realtime
        Duration::from_millis(15_000 + audio_ms)
    }
}

pub struct WhisperOnnxProvider {
    id: String,
    dir: PathBuf,
    /// The loaded recognizer and the language it was created for ("" = detect).
    model: Arc<Mutex<Option<(String, sherpa_rs::whisper::WhisperRecognizer)>>>,
}

impl WhisperOnnxProvider {
    pub fn new(id: &str, dir: PathBuf) -> Self {
        Self { id: id.into(), dir, model: Arc::new(Mutex::new(None)) }
    }

    pub fn is_installed(&self) -> bool {
        find(&self.dir, "", "encoder.int8.onnx").or_else(|| find(&self.dir, "", "encoder.onnx")).is_some()
    }

    fn load(dir: &Path, language: &str) -> Result<sherpa_rs::whisper::WhisperRecognizer, ProviderError> {
        let missing = || ProviderError::ModelMissing("Whisper".into());
        let encoder = find(dir, "", "encoder.int8.onnx").or_else(|| find(dir, "", "encoder.onnx")).ok_or_else(missing)?;
        let decoder = find(dir, "", "decoder.int8.onnx").or_else(|| find(dir, "", "decoder.onnx")).ok_or_else(missing)?;
        let tokens = find(dir, "", "tokens.txt").ok_or_else(missing)?;
        let cfg = sherpa_rs::whisper::WhisperConfig {
            encoder: encoder.to_string_lossy().into(),
            decoder: decoder.to_string_lossy().into(),
            tokens: tokens.to_string_lossy().into(),
            language: language.into(),
            num_threads: Some(threads()),
            ..Default::default()
        };
        sherpa_rs::whisper::WhisperRecognizer::new(cfg)
            .map_err(|e| ProviderError::Other(format!("Whisper failed to load: {e}")))
    }
}

#[async_trait]
impl AsrProvider for WhisperOnnxProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn is_local(&self) -> bool {
        true
    }
    async fn transcribe(&self, req: &AsrRequest) -> Result<String, ProviderError> {
        let model = self.model.clone();
        let dir = self.dir.clone();
        let pcm = req.pcm.clone();
        let language = req.language.clone();
        tokio::task::spawn_blocking(move || {
            let mut g = model.lock().map_err(|_| ProviderError::Other("model lock poisoned".into()))?;
            if g.as_ref().map(|(l, _)| *l != language).unwrap_or(true) {
                *g = Some((language.clone(), Self::load(&dir, &language)?));
            }
            let r = &mut g.as_mut().unwrap().1;
            // Whisper ONNX handles ≤ 30 s per call; split longer audio.
            let samples = to_f32(&pcm);
            let mut text = String::new();
            for chunk in samples.chunks(16_000 * 28) {
                let t = r.transcribe(16_000, chunk).text;
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(t.trim());
            }
            Ok(text)
        })
        .await
        .map_err(|e| ProviderError::Other(e.to_string()))?
    }
    fn timeout(&self, audio_ms: u64) -> Duration {
        Duration::from_millis(20_000 + audio_ms * 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models_dir() -> Option<PathBuf> {
        let base = std::env::var_os("LOCALAPPDATA")?;
        let p = PathBuf::from(base).join("dev.dettino.desktop").join("models");
        p.exists().then_some(p)
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn fills_only_gated_gaps() {
        let mut x = vec![0.1f32; 1000];
        x[100..100 + FRAME].fill(0.0); // one frame of digital silence: filled
        x[500..500 + FRAME - 1].fill(0.0); // shorter: a zero crossing, left alone
        fill_digital_silence(&mut x);
        let gap = &x[100..100 + FRAME];
        assert!(gap.iter().all(|&s| s != 0.0 && s.abs() < 10.0 / 32768.0));
        assert!((rms(gap) - GAP_NOISE_RMS).abs() < GAP_NOISE_RMS * 0.3);
        assert!(x[500..500 + FRAME - 1].iter().all(|&s| s == 0.0));
        assert!(x[..100].iter().all(|&s| s == 0.1));
    }

    #[test]
    fn pieces_cover_the_take_and_cut_in_pauses() {
        let sr = 16_000;
        assert_eq!(pieces(&vec![0.1f32; sr * 6]), vec![0..sr * 6]);
        // 14 s of "speech" with a quiet 200 ms at 4 s and at 8 s.
        let mut x = vec![0.1f32; sr * 14];
        x[sr * 4..sr * 4 + sr / 5].fill(0.001);
        x[sr * 8..sr * 8 + sr / 5].fill(0.001);
        let p = pieces(&x);
        assert_eq!(p.first().unwrap().start, 0);
        assert_eq!(p.last().unwrap().end, x.len());
        assert!(p.windows(2).all(|w| w[0].end == w[1].start));
        assert_eq!(p.len(), 3);
        for cut in [p[0].end, p[1].end] {
            assert!(x[cut] < 0.01, "cut at {cut} is not in a pause");
        }
    }

    /// Runs only when the Parakeet model has been downloaded.
    #[tokio::test]
    async fn parakeet_transcribes_test_wav() {
        let Some(models) = models_dir() else { return };
        let dir = models.join("parakeet");
        let p = ParakeetProvider::new("parakeet", dir.clone());
        if !p.is_installed() {
            return;
        }
        let wav = find(&dir, "0", ".wav").or_else(|| {
            std::fs::read_dir(&dir).ok()?.flatten().find_map(|e| find(&e.path().join("test_wavs"), "0", ".wav"))
        });
        let Some(wav) = wav else { return };
        let pcm = app_audio::wav::read(&wav).unwrap();
        let req = AsrRequest { pcm: Arc::new(pcm), language: "en".into(), hotwords: vec![], prompt_tail: None };
        let t0 = std::time::Instant::now();
        let text = p.transcribe(&req).await.unwrap();
        let cold = t0.elapsed();
        let t1 = std::time::Instant::now();
        let text2 = p.transcribe(&req).await.unwrap();
        let warm = t1.elapsed();
        eprintln!("parakeet: {text:?} cold={cold:?} warm={warm:?} audio_ms={}", req.audio_ms());
        assert!(!text.is_empty());
        assert_eq!(text, text2);
    }
}
