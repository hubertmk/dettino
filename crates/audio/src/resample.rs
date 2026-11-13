//! Streaming windowed-sinc resampler (any rate → 16 kHz mono).
//!
//! Quality target is speech recognition: −60 dB stop band is plenty. The kernel
//! is a Blackman-windowed sinc with the cutoff at 0.45 × the output rate.

/// Fractional-position resolution of the kernel table. Between two neighbouring
/// phases the taps are interpolated, so the effective resolution is much finer.
const PHASES: usize = 256;

pub struct Resampler {
    in_rate: f64,
    out_rate: f64,
    step: f64,
    half_taps: usize,
    /// Windowed-sinc taps for each fractional position: `(PHASES + 1) × 2·half_taps`.
    /// Computed once; the per-sample loop is then multiply-adds only (the kernel used
    /// to cost about 2.3 million trigonometric calls per second with a warm microphone).
    table: Vec<f32>,
    /// Input history (keeps `half_taps` samples of look-behind).
    buf: Vec<f32>,
    /// Position of the next output sample, in input-sample units relative to `buf[0]`.
    pos: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let in_rate = in_rate as f64;
        let out_rate = out_rate as f64;
        let half_taps = 24;
        let cutoff = (0.45 * out_rate / in_rate).min(0.45);
        let taps = 2 * half_taps;
        let mut table = Vec::with_capacity((PHASES + 1) * taps);
        for ph in 0..=PHASES {
            let frac = ph as f64 / PHASES as f64;
            for k in 0..taps {
                // tap k sits at input index floor(pos) - half_taps + 1 + k, i.e. at
                // x = (k - half_taps + 1) - frac from the output position
                let x = (k as f64 - half_taps as f64 + 1.0) - frac;
                table.push(kernel(x, half_taps as f64, cutoff) as f32);
            }
        }
        Self { in_rate, out_rate, step: in_rate / out_rate, half_taps, table, buf: vec![0.0; half_taps], pos: half_taps as f64 }
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate as u32
    }

    /// Feed input samples; returns resampled output.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.in_rate - self.out_rate).abs() < 0.5 {
            return input.to_vec();
        }
        self.buf.extend_from_slice(input);
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 2);
        let n = self.half_taps as f64;
        let taps = 2 * self.half_taps;
        while self.pos + n < self.buf.len() as f64 {
            let fc = self.pos.floor();
            let frac = self.pos - fc;
            let base = fc as usize + 1 - self.half_taps;
            let ph = frac * PHASES as f64;
            let p0 = (ph.floor() as usize).min(PHASES - 1);
            let t = (ph - p0 as f64) as f32;
            let row0 = &self.table[p0 * taps..(p0 + 1) * taps];
            let row1 = &self.table[(p0 + 1) * taps..(p0 + 2) * taps];
            let window = &self.buf[base..base + taps];
            let mut acc = 0.0f32;
            for k in 0..taps {
                acc += window[k] * (row0[k] + (row1[k] - row0[k]) * t);
            }
            out.push(acc);
            self.pos += self.step;
        }
        // drop consumed input, keep look-behind
        let keep_from = (self.pos - n).floor().max(0.0) as usize;
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
        out
    }
}

/// Blackman-windowed sinc, `half_taps` wide on each side.
fn kernel(x: f64, n: f64, cutoff: f64) -> f64 {
    if x.abs() >= n {
        return 0.0;
    }
    let sinc = if x.abs() < 1e-9 { 2.0 * cutoff } else { (2.0 * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x) };
    let t = (x + n) / (2.0 * n);
    let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos() + 0.08 * (4.0 * std::f64::consts::PI * t).cos();
    sinc * w
}

pub fn f32_to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn length_ratio_48k_to_16k() {
        let mut r = Resampler::new(48_000, 16_000);
        let mut total = 0;
        for chunk in sine(48_000, 440.0, 1.0).chunks(480) {
            total += r.process(chunk).len();
        }
        assert!((total as i32 - 16_000).abs() < 40, "got {total}");
    }

    #[test]
    fn passband_preserved_and_alias_rejected() {
        let mut r = Resampler::new(48_000, 16_000);
        let out = r.process(&sine(48_000, 1000.0, 0.5));
        let tone = rms(&out[200..]);
        assert!((tone - 0.5 / 2f32.sqrt()).abs() < 0.03, "passband rms {tone}");

        let mut r = Resampler::new(48_000, 16_000);
        let out = r.process(&sine(48_000, 12_000.0, 0.5));
        let alias = rms(&out[200..]);
        assert!(alias < 0.01, "alias rms {alias}");
    }

    #[test]
    fn non_integer_ratio() {
        let mut r = Resampler::new(44_100, 16_000);
        let mut total = 0;
        for chunk in sine(44_100, 300.0, 1.0).chunks(441) {
            total += r.process(chunk).len();
        }
        assert!((total as i32 - 16_000).abs() < 40, "got {total}");
    }

    #[test]
    fn passthrough_at_16k() {
        let mut r = Resampler::new(16_000, 16_000);
        assert_eq!(r.process(&[0.1, 0.2]), vec![0.1, 0.2]);
    }
}
