//! Voice activity detection interface. The Silero implementation lives in
//! `app-asr-local` (sherpa-onnx); this crate ships an energy-based fallback that
//! is used when the Silero model is not downloaded yet.

/// Fed with 16 kHz mono f32 in arbitrary chunk sizes.
pub trait Vad: Send {
    /// Returns true when the detector currently considers the input to be speech.
    fn accept(&mut self, samples: &[f32]) -> bool;
    fn reset(&mut self);
    fn name(&self) -> &'static str;
}

/// Energy VAD: speech when a 20 ms window is ≥ 12 dB above the running floor,
/// with a 300 ms hangover.
pub struct EnergyVad {
    floor_db: f32,
    hang: usize,
    pending: Vec<f32>,
    speech: bool,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self { floor_db: -55.0, hang: 0, pending: Vec::new(), speech: false }
    }
}

const WIN: usize = 320; // 20 ms at 16 kHz

impl Vad for EnergyVad {
    fn accept(&mut self, samples: &[f32]) -> bool {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= WIN {
            let w: Vec<f32> = self.pending.drain(..WIN).collect();
            let db = crate::level::rms_dbfs(&w);
            if db < self.floor_db {
                self.floor_db = 0.8 * self.floor_db + 0.2 * db;
            } else {
                self.floor_db += 0.02;
            }
            self.floor_db = self.floor_db.clamp(-80.0, -35.0);
            if db > self.floor_db + 12.0 && db > -50.0 {
                self.speech = true;
                self.hang = 15; // 300 ms
            } else if self.hang > 0 {
                self.hang -= 1;
            } else {
                self.speech = false;
            }
        }
        self.speech
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.speech = false;
        self.hang = 0;
    }

    fn name(&self) -> &'static str {
        "energy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tone_after_silence() {
        let mut v = EnergyVad::default();
        assert!(!v.accept(&vec![0.0005; 16_000]));
        let tone: Vec<f32> = (0..3200).map(|i| (i as f32 * 0.2).sin() * 0.2).collect();
        assert!(v.accept(&tone));
        // hangover then silence
        assert!(v.accept(&vec![0.0005; 1600]));
        assert!(!v.accept(&vec![0.0005; 16_000]));
    }
}
