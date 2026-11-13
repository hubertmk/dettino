//! Level meter: RMS dBFS per 40 ms frame. The bar starts a little above the
//! noise floor (so room noise reads as zero) and is full at loud speech.
//! A purely floor-relative scale pegged on quiet Bluetooth mics (typing filled
//! it) and on noisy webcams (room noise filled it).

use std::collections::VecDeque;

/// Floor = quietest frame of the last 3 s (75 × 40 ms frames).
const FLOOR_WINDOW: usize = 75;
/// The bar opens this far above the floor...
const GATE_ABOVE_FLOOR_DB: f32 = 6.0;
/// ...but never below this, so near-silent mics do not amplify keyboard clicks.
const MIN_GATE_DB: f32 = -55.0;
/// Full scale: loud speech close to the mic.
const FULL_DB: f32 = -10.0;

pub fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }
    let ms = samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32;
    if ms <= 1e-12 {
        -100.0
    } else {
        10.0 * ms.log10()
    }
}

#[derive(Default)]
pub struct LevelMeter {
    recent: VecDeque<f32>,
}

impl LevelMeter {
    /// Returns (level 0..1, dBFS).
    pub fn push(&mut self, frame: &[f32]) -> (f32, f32) {
        let db = rms_dbfs(frame);
        if self.recent.len() == FLOOR_WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(db);
        let gate = (self.floor_db() + GATE_ABOVE_FLOOR_DB).max(MIN_GATE_DB);
        let level = ((db - gate) / (FULL_DB - gate)).clamp(0.0, 1.0);
        (level, db)
    }

    /// Noise floor estimate, −90..−30 dBFS.
    pub fn floor_db(&self) -> f32 {
        self.recent.iter().copied().fold(f32::INFINITY, f32::min).clamp(-90.0, -30.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_loud() {
        let mut m = LevelMeter::default();
        let (l, db) = m.push(&[0.0; 640]);
        assert_eq!(l, 0.0);
        assert!(db <= -99.0);
        let loud: Vec<f32> = (0..640).map(|i| if i % 2 == 0 { 0.3 } else { -0.3 }).collect();
        let (l, db) = m.push(&loud);
        assert!(l > 0.9, "{l}");
        assert!((db - (-10.46)).abs() < 0.2, "{db}");
    }

    fn tone(dbfs: f32) -> Vec<f32> {
        // square wave: RMS == amplitude
        let a = 10f32.powf(dbfs / 20.0);
        (0..640).map(|i| if i % 2 == 0 { a } else { -a }).collect()
    }

    #[test]
    fn noisy_webcam_room_noise_reads_zero() {
        let mut m = LevelMeter::default();
        for _ in 0..FLOOR_WINDOW {
            m.push(&tone(-41.0));
        }
        assert_eq!(m.push(&tone(-40.0)).0, 0.0);
        let (speech, _) = m.push(&tone(-20.0));
        assert!((0.4..0.8).contains(&speech), "{speech}");
    }

    #[test]
    fn quiet_mic_typing_stays_low() {
        let mut m = LevelMeter::default();
        for _ in 0..FLOOR_WINDOW {
            m.push(&tone(-75.0));
        }
        let (typing, _) = m.push(&tone(-42.0));
        assert!(typing < 0.35, "{typing}");
        let (speech, _) = m.push(&tone(-20.0));
        assert!(speech > 0.7, "{speech}");
    }
}
