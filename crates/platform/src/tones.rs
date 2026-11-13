//! The start/stop/hands-free/error sounds as an in-memory WAV, shared by the platform players.
//! Every style is loudness-matched to Classic, so the volume slider means the same for all.
#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

use crate::SoundKind;
use app_core::SoundStyle;
use std::f32::consts::{PI, TAU};

const RATE: u32 = 48_000;
/// −22 dBFS for Classic at full volume slider; the slider scales further.
const LEVEL: f32 = 0.079;

/// Classic: (freq, ms) segments with flat notes; 0 Hz is a gap.
fn classic_segments(kind: SoundKind) -> &'static [(f32, u32)] {
    match kind {
        SoundKind::Start => &[(880.0, 50), (0.0, 15), (1175.0, 50)],
        SoundKind::Stop => &[(1175.0, 50), (0.0, 15), (880.0, 50)],
        SoundKind::HandsFree => &[(880.0, 40), (0.0, 15), (1047.0, 40), (0.0, 15), (1319.0, 40)],
        SoundKind::Error => &[(220.0, 120)],
    }
}

fn classic(kind: SoundKind, volume: f32) -> Vec<f32> {
    let amp = LEVEL * volume * if kind == SoundKind::Error { 0.6 } else { 1.0 };
    // 5 ms fade in/out to avoid clicks
    let fade = (RATE as usize * 5 / 1000).max(1);
    let mut out = Vec::new();
    for &(freq, ms) in classic_segments(kind) {
        let n = (RATE * ms / 1000) as usize;
        for i in 0..n {
            let env = (i.min(n - 1 - i) as f32 / fade as f32).min(1.0);
            let v = if freq == 0.0 { 0.0 } else { (TAU * freq * i as f32 / RATE as f32).sin() * amp * env };
            out.push(v);
        }
    }
    out
}

/// A struck voice: partials as (frequency ratio, amplitude, decay τ in ms), a raised-cosine
/// attack, and an optional downward pitch flick (cents, settling in ~6 ms) at the onset.
struct Voice {
    partials: &'static [(f32, f32, f32)],
    attack_ms: f32,
    flick_cents: f32,
}

fn voice(style: SoundStyle) -> Option<Voice> {
    Some(match style {
        SoundStyle::Classic => return None,
        SoundStyle::SoftPluck => Voice { partials: &[(1.0, 1.0, 40.0), (2.0, 0.2, 20.0)], attack_ms: 1.5, flick_cents: 0.0 },
        SoundStyle::Mallet => {
            Voice { partials: &[(1.0, 1.0, 70.0), (2.0, 0.25, 35.0), (3.9, 0.06, 12.0)], attack_ms: 2.0, flick_cents: 0.0 }
        }
        SoundStyle::Glass => Voice {
            partials: &[(1.0, 1.0, 120.0), (2.0, 0.15, 80.0), (3.0, 0.08, 50.0), (4.2, 0.03, 25.0)],
            attack_ms: 4.0,
            flick_cents: 0.0,
        },
        SoundStyle::WarmPop => Voice { partials: &[(1.0, 1.0, 45.0), (2.0, 0.08, 20.0)], attack_ms: 1.0, flick_cents: 300.0 },
    })
}

/// Notes as (Hz, onset ms, gain). They overlap, so each cue is one gesture.
fn notes(style: SoundStyle, kind: SoundKind) -> &'static [(f32, f32, f32)] {
    use SoundKind::*;
    match (style, kind) {
        (SoundStyle::Classic, _) => &[],
        (SoundStyle::SoftPluck, Start) => &[(880.0, 0.0, 1.0), (1175.0, 45.0, 0.9)],
        (SoundStyle::SoftPluck, Stop) => &[(1175.0, 0.0, 1.0), (880.0, 45.0, 0.9)],
        (SoundStyle::SoftPluck, HandsFree) => &[(880.0, 0.0, 1.0), (1047.0, 40.0, 0.9), (1319.0, 80.0, 0.85)],
        (SoundStyle::SoftPluck, Error) => &[(440.0, 0.0, 1.0), (415.0, 80.0, 1.0)],
        (SoundStyle::Mallet, Start) => &[(659.0, 0.0, 1.0), (880.0, 60.0, 0.9)],
        (SoundStyle::Mallet, Stop) => &[(880.0, 0.0, 1.0), (659.0, 60.0, 0.9)],
        (SoundStyle::Mallet, HandsFree) => &[(659.0, 0.0, 1.0), (880.0, 55.0, 0.9), (1109.0, 110.0, 0.85)],
        (SoundStyle::Mallet, Error) => &[(392.0, 0.0, 1.0), (370.0, 90.0, 1.0)],
        (SoundStyle::Glass, Start) => &[(880.0, 0.0, 1.0), (1319.0, 70.0, 0.8)],
        (SoundStyle::Glass, Stop) => &[(1319.0, 0.0, 0.8), (880.0, 70.0, 1.0)],
        (SoundStyle::Glass, HandsFree) => &[(880.0, 0.0, 1.0), (1109.0, 60.0, 0.85), (1319.0, 120.0, 0.8)],
        (SoundStyle::Glass, Error) => &[(523.0, 0.0, 1.0), (494.0, 100.0, 1.0)],
        (SoundStyle::WarmPop, Start) => &[(587.0, 0.0, 1.0), (784.0, 55.0, 0.9)],
        (SoundStyle::WarmPop, Stop) => &[(784.0, 0.0, 1.0), (587.0, 55.0, 0.9)],
        (SoundStyle::WarmPop, HandsFree) => &[(587.0, 0.0, 1.0), (784.0, 50.0, 0.9), (988.0, 100.0, 0.85)],
        (SoundStyle::WarmPop, Error) => &[(330.0, 0.0, 1.0), (311.0, 90.0, 1.0)],
    }
}

fn note(freq: f32, v: &Voice, tau_scale: f32) -> Vec<f32> {
    let rate = RATE as f32;
    let longest = v.partials.iter().map(|p| p.2).fold(0.0, f32::max) * tau_scale;
    let n = (rate * (5.0 * longest).min(400.0) / 1000.0) as usize;
    let mut out = vec![0.0f32; n];
    for &(ratio, amp, tau) in v.partials {
        let tau_s = tau * tau_scale / 1000.0;
        let mut phase = 0.0f32;
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / rate;
            let cents = if v.flick_cents != 0.0 { v.flick_cents * (-t / 0.006).exp() } else { 0.0 };
            phase = (phase + TAU * freq * ratio * 2f32.powf(cents / 1200.0) / rate) % TAU;
            *o += phase.sin() * amp * (-t / tau_s).exp();
        }
    }
    let attack = ((rate * v.attack_ms / 1000.0) as usize).max(1);
    for (i, o) in out.iter_mut().take(attack).enumerate() {
        *o *= 0.5 - 0.5 * (PI * i as f32 / attack as f32).cos();
    }
    // 10 ms fade at the tail so it never ends on a click
    let fade = RATE as usize / 100;
    for (k, o) in out.iter_mut().rev().take(fade).enumerate() {
        *o *= k as f32 / fade as f32;
    }
    out
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn samples(kind: SoundKind, style: SoundStyle, volume: f32) -> Vec<f32> {
    let volume = volume.clamp(0.0, 1.0);
    let reference = classic(kind, volume);
    let Some(v) = voice(style) else { return reference };
    let tau_scale = if kind == SoundKind::Stop { 0.8 } else { 1.0 };
    let mut out: Vec<f32> = Vec::new();
    for &(freq, onset_ms, gain) in notes(style, kind) {
        let start = (RATE as f32 * onset_ms / 1000.0) as usize;
        let x = note(freq, &v, tau_scale);
        if out.len() < start + x.len() {
            out.resize(start + x.len(), 0.0);
        }
        for (o, s) in out[start..].iter_mut().zip(x) {
            *o += s * gain;
        }
    }
    // Match Classic's loudness over Classic's length; errors sit a little louder than Classic's.
    let win = reference.len().min(out.len());
    let mut scale = rms(&reference) / rms(&out[..win]).max(1e-9);
    if kind == SoundKind::Error {
        scale *= 0.8 / 0.6;
    }
    out.iter_mut().for_each(|s| *s *= scale);
    out
}

pub(crate) fn wav(kind: SoundKind, style: SoundStyle, volume: f32) -> Vec<u8> {
    let pcm: Vec<i16> = samples(kind, style, volume).iter().map(|v| (v * 32767.0) as i16).collect();
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [SoundStyle; 5] =
        [SoundStyle::Classic, SoundStyle::SoftPluck, SoundStyle::Mallet, SoundStyle::Glass, SoundStyle::WarmPop];
    const KINDS: [SoundKind; 4] = [SoundKind::Start, SoundKind::Stop, SoundKind::HandsFree, SoundKind::Error];

    #[test]
    fn wav_header_matches_data() {
        for style in STYLES {
            for kind in KINDS {
                let b = wav(kind, style, 1.0);
                assert_eq!(&b[0..4], b"RIFF");
                let data_len = u32::from_le_bytes(b[40..44].try_into().unwrap()) as usize;
                assert_eq!(b.len(), 44 + data_len);
            }
        }
    }

    #[test]
    fn styles_are_as_loud_as_classic_and_never_clip() {
        for kind in [SoundKind::Start, SoundKind::Stop, SoundKind::HandsFree] {
            let reference = rms(&classic(kind, 1.0));
            for style in STYLES {
                let x = samples(kind, style, 1.0);
                let win = classic(kind, 1.0).len().min(x.len());
                assert!((rms(&x[..win]) / reference - 1.0).abs() < 0.01, "{style:?} {kind:?}");
                assert!(x.iter().all(|v| v.abs() < 0.5), "{style:?} {kind:?}");
                assert!(x.last().unwrap().abs() < 1e-3, "{style:?} {kind:?} ends on a click");
            }
        }
    }

    /// `DETTINO_TONES_DIR=/tmp/t cargo test -p app-platform tones -- --ignored`, then `afplay`.
    #[test]
    #[ignore]
    fn write_audition_files() {
        let dir = std::env::var("DETTINO_TONES_DIR").expect("set DETTINO_TONES_DIR");
        std::fs::create_dir_all(&dir).unwrap();
        for style in STYLES {
            for kind in KINDS {
                std::fs::write(format!("{dir}/{style:?}-{kind:?}.wav").to_lowercase(), wav(kind, style, 0.3)).unwrap();
            }
        }
    }
}
