//! Silero VAD (via sherpa-onnx) behind the audio crate's `Vad` trait.

use app_audio::Vad;
use std::path::Path;

pub struct SileroVad {
    inner: sherpa_rs::silero_vad::SileroVad,
    pending: Vec<f32>,
    speech: bool,
}

const WINDOW: usize = 512;

impl SileroVad {
    pub fn new(model: &Path) -> Result<Self, String> {
        if !model.exists() {
            return Err(format!("{} not found", model.display()));
        }
        let cfg = sherpa_rs::silero_vad::SileroVadConfig {
            model: model.to_string_lossy().into(),
            min_silence_duration: 0.4,
            min_speech_duration: 0.1,
            max_speech_duration: 30.0,
            threshold: 0.5,
            sample_rate: 16_000,
            window_size: WINDOW as i32,
            num_threads: Some(1),
            ..Default::default()
        };
        let inner = sherpa_rs::silero_vad::SileroVad::new(cfg, 60.0).map_err(|e| e.to_string())?;
        Ok(Self { inner, pending: Vec::new(), speech: false })
    }
}

impl Vad for SileroVad {
    fn accept(&mut self, samples: &[f32]) -> bool {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= WINDOW {
            let w: Vec<f32> = self.pending.drain(..WINDOW).collect();
            self.inner.accept_waveform(w);
            self.speech = self.inner.is_speech();
            // Completed segments are not needed; free them.
            while !self.inner.is_empty() {
                self.inner.pop();
            }
        }
        self.speech
    }

    fn reset(&mut self) {
        self.inner.clear();
        self.pending.clear();
        self.speech = false;
    }

    fn name(&self) -> &'static str {
        "silero"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_not_speech() {
        let Some(base) = std::env::var_os("LOCALAPPDATA") else { return };
        let model = std::path::PathBuf::from(base).join("dev.dettino.desktop/models/vad/silero_vad.onnx");
        let Ok(mut v) = SileroVad::new(&model) else { return };
        assert!(!v.accept(&vec![0.0; 16_000]));
    }
}
