//! Capture engine: owns the cpal stream on a dedicated worker thread, resamples
//! to 16 kHz mono, keeps the pre-roll ring, meters levels, runs the VAD and
//! writes every session to disk as it is recorded.

use crate::level::LevelMeter;
use crate::resample::{f32_to_i16, Resampler};
use crate::vad::{EnergyVad, Vad};
use crate::wav::WavWriter;
use app_core::{MicEarlyOpen, MicMode};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, InterfaceType, SampleFormat};
use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const RATE: usize = 16_000;
const FRAME: usize = 640; // 40 ms
const RING_SECS: usize = 2;

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub device: Option<String>,
    pub mode: MicMode,
    pub early_open: MicEarlyOpen,
    pub preroll_ms: u64,
    pub prefer_wired: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { device: None, mode: MicMode::AlwaysWarm, early_open: MicEarlyOpen::Off, preroll_ms: 1500, prefer_wired: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    BuiltIn,
    Usb,
    Bluetooth,
    Virtual,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// Level of the last 40 ms frame (only while a session or monitoring is active).
    Level { level: f32, db: f32, speech: bool },
    /// First live speech in a session (pre-roll speech does not count).
    Speech { session: u64 },
    /// No signal for 3 s in a session.
    NoSignal { session: u64, device: String },
    /// Stream (re)opened on this device.
    DeviceActive { name: String, kind: DeviceKind },
    StreamClosed,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct Take {
    pub session: u64,
    pub pcm: Vec<i16>,
    pub preroll_samples: usize,
    /// Sample index of first / last detected speech (including pre-roll).
    pub speech_start: Option<usize>,
    pub speech_end: Option<usize>,
    pub max_db: f32,
    pub wav_path: PathBuf,
    pub device: String,
    pub vad: &'static str,
}

impl Take {
    pub fn audio_ms(&self) -> u64 {
        (self.pcm.len() * 1000 / RATE) as u64
    }

    pub fn speech_ms(&self) -> u64 {
        match (self.speech_start, self.speech_end) {
            (Some(a), Some(b)) if b > a => ((b - a) * 1000 / RATE) as u64,
            _ => 0,
        }
    }

    /// Whether anything worth transcribing was captured.
    pub fn has_speech(&self) -> bool {
        self.speech_start.is_some() || (self.vad == "energy" && self.max_db > -45.0)
    }

    /// Audio trimmed to speech with margins (300 ms before, 400 ms after).
    pub fn trimmed(&self) -> &[i16] {
        let len = self.pcm.len();
        let start = match self.speech_start {
            Some(s) => s.saturating_sub(RATE * 3 / 10),
            None => self.preroll_samples.min(len),
        };
        let end = match self.speech_end {
            Some(e) => (e + RATE * 4 / 10).min(len),
            None => len,
        };
        if start >= end {
            &self.pcm[..]
        } else {
            &self.pcm[start..end]
        }
    }
}

enum Cmd {
    Begin { session: u64, wav_path: PathBuf, reply: Sender<Result<(), String>> },
    Finish { session: u64, reply: Sender<Option<Take>> },
    Discard { session: u64 },
    Warm,
    Cool,
    Monitor(bool),
    SetConfig(AudioConfig),
    SetVadFactory(Box<dyn Fn() -> Box<dyn Vad> + Send>),
    Pause(bool),
    Shutdown,
}

#[derive(Clone)]
pub struct AudioEngine {
    tx: Sender<Cmd>,
}

impl AudioEngine {
    pub fn start(cfg: AudioConfig) -> (Self, Receiver<AudioEvent>) {
        let (tx, rx) = unbounded();
        let (ev_tx, ev_rx) = unbounded();
        std::thread::Builder::new()
            .name("audio-worker".into())
            .spawn(move || Worker::new(cfg, ev_tx).run(rx))
            .expect("spawn audio worker");
        (Self { tx }, ev_rx)
    }

    /// Start recording a session (pre-roll is included).
    pub fn begin(&self, session: u64, wav_path: PathBuf) -> Result<(), String> {
        let (reply, r) = bounded(1);
        self.tx.send(Cmd::Begin { session, wav_path, reply }).map_err(|e| e.to_string())?;
        r.recv_timeout(Duration::from_secs(3)).map_err(|e| e.to_string())?
    }

    /// Stop recording and get the take. The WAV is finalized on disk.
    pub fn finish(&self, session: u64) -> Option<Take> {
        let (reply, r) = bounded(1);
        self.tx.send(Cmd::Finish { session, reply }).ok()?;
        r.recv_timeout(Duration::from_secs(5)).ok().flatten()
    }

    /// Throw a session away (tap): the WAV is deleted.
    pub fn discard(&self, session: u64) {
        let _ = self.tx.send(Cmd::Discard { session });
    }

    pub fn warm(&self) {
        let _ = self.tx.send(Cmd::Warm);
    }

    /// The modifier that warmed the mic did not start a chord.
    pub fn cool(&self) {
        let _ = self.tx.send(Cmd::Cool);
    }

    /// Emit levels even without a session (mic test).
    pub fn monitor(&self, on: bool) {
        let _ = self.tx.send(Cmd::Monitor(on));
    }

    pub fn set_config(&self, cfg: AudioConfig) {
        let _ = self.tx.send(Cmd::SetConfig(cfg));
    }

    pub fn set_vad_factory(&self, f: Box<dyn Fn() -> Box<dyn Vad> + Send>) {
        let _ = self.tx.send(Cmd::SetVadFactory(f));
    }

    pub fn pause(&self, paused: bool) {
        let _ = self.tx.send(Cmd::Pause(paused));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().map(|d| d.to_string());
    let mut out = Vec::new();
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            let name = d.to_string();
            out.push(DeviceInfo {
                kind: device_kind(&d),
                is_default: Some(&name) == default_name.as_ref(),
                name,
            });
        }
    }
    out
}

fn device_kind(d: &cpal::Device) -> DeviceKind {
    let name = d.to_string().to_lowercase();
    if ["cable", "voicemeeter", "virtual", "blackhole", "krisp", "broadcast"].iter().any(|k| name.contains(k)) {
        return DeviceKind::Virtual;
    }
    match d.description().map(|x| x.interface_type()) {
        Ok(InterfaceType::Bluetooth) => DeviceKind::Bluetooth,
        Ok(InterfaceType::Usb) => DeviceKind::Usb,
        Ok(InterfaceType::BuiltIn) | Ok(InterfaceType::Pci) | Ok(InterfaceType::Line) => DeviceKind::BuiltIn,
        _ => {
            if name.contains("bluetooth") || name.contains("airpods") || name.contains("hands-free") {
                DeviceKind::Bluetooth
            } else {
                DeviceKind::Other
            }
        }
    }
}

struct Session {
    id: u64,
    pcm: Vec<i16>,
    preroll_samples: usize,
    speech_start: Option<usize>,
    speech_end: Option<usize>,
    live_speech_reported: bool,
    max_db: f32,
    wav: Option<WavWriter>,
    wav_path: PathBuf,
    vad: Box<dyn Vad>,
    started: Instant,
    no_signal_reported: bool,
}

struct Open {
    _stream: cpal::Stream,
    consumer: rtrb::Consumer<f32>,
    resampler: Resampler,
    broken: Arc<AtomicBool>,
    name: String,
    using_default: bool,
    last_data: Instant,
}

struct Worker {
    cfg: AudioConfig,
    events: Sender<AudioEvent>,
    open: Option<Open>,
    ring: VecDeque<f32>,
    session: Option<Session>,
    meter: LevelMeter,
    frame_acc: Vec<f32>,
    monitor: bool,
    paused: bool,
    /// Start of the keep-open window; `None` once a warm-up was cancelled.
    last_used: Option<Instant>,
    last_default_check: Instant,
    vad_factory: Box<dyn Fn() -> Box<dyn Vad> + Send>,
    reopen_backoff_until: Option<Instant>,
    /// A device change arrived during a session; applied when it ends.
    pending_reopen: bool,
}

impl Worker {
    fn new(cfg: AudioConfig, events: Sender<AudioEvent>) -> Self {
        Self {
            cfg,
            events,
            open: None,
            ring: VecDeque::with_capacity(RATE * RING_SECS),
            session: None,
            meter: LevelMeter::default(),
            frame_acc: Vec::with_capacity(FRAME),
            monitor: false,
            paused: false,
            last_used: Some(Instant::now()),
            last_default_check: Instant::now(),
            vad_factory: Box::new(|| Box::new(EnergyVad::default())),
            reopen_backoff_until: None,
            pending_reopen: false,
        }
    }

    fn emit(&self, e: AudioEvent) {
        let _ = self.events.send(e);
    }

    fn should_be_open(&self) -> bool {
        if self.paused {
            return false;
        }
        if self.session.is_some() || self.monitor {
            return true;
        }
        let used_within = |secs| self.last_used.is_some_and(|t| t.elapsed() < Duration::from_secs(secs));
        match self.cfg.mode {
            MicMode::AlwaysWarm => true,
            MicMode::WarmOnModifier => used_within(120),
            // With early open, `warm()` on modifier-down opens it a moment before the chord completes.
            MicMode::OnDemand => used_within(3),
        }
    }

    fn pick_device(&self, host: &cpal::Host) -> Option<(cpal::Device, bool)> {
        if let Some(want) = &self.cfg.device {
            if let Ok(devs) = host.input_devices() {
                for d in devs {
                    if &d.to_string() == want {
                        return Some((d, false));
                    }
                }
            }
            tracing::warn!("configured input device not found, using default");
        }
        let default = host.default_input_device()?;
        if self.cfg.prefer_wired && device_kind(&default) == DeviceKind::Bluetooth {
            if let Ok(devs) = host.input_devices() {
                let mut alts: Vec<cpal::Device> = devs
                    .filter(|d| matches!(device_kind(d), DeviceKind::BuiltIn | DeviceKind::Usb))
                    .collect();
                alts.sort_by_key(|d| if device_kind(d) == DeviceKind::Usb { 0 } else { 1 });
                if let Some(d) = alts.into_iter().next() {
                    return Some((d, false));
                }
            }
        }
        Some((default, true))
    }

    fn open_stream(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let (device, using_default) = self.pick_device(&host).ok_or("No microphone found")?;
        let name = device.to_string();
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = supported.channels() as usize;
        let rate = supported.sample_rate();
        let config = supported.config();
        let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(rate as usize * 2);
        let broken = Arc::new(AtomicBool::new(false));
        let b2 = broken.clone();
        let err_cb = move |e: cpal::Error| {
            // Glitches and automatic reroutes leave the stream running; rebuilding on them
            // loops forever on devices that report a discontinuity right after start.
            if matches!(e.kind(), ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied) {
                tracing::debug!("audio stream glitch: {e}");
                return;
            }
            tracing::warn!("audio stream error: {e}");
            b2.store(true, Ordering::SeqCst);
        };

        macro_rules! build {
            ($t:ty, $conv:expr) => {{
                device.build_input_stream::<$t, _, _>(
                    config.clone(),
                    move |data: &[$t], _| {
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().map(|&s| $conv(s)).sum();
                            let _ = producer.push(sum / channels as f32);
                        }
                    },
                    err_cb,
                    None,
                )
            }};
        }
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build!(f32, |s: f32| s),
            SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32768.0),
            SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2147483648.0),
            SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
            SampleFormat::F64 => build!(f64, |s: f64| s as f32),
            other => return Err(format!("unsupported sample format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let kind = device_kind(&device);
        tracing::info!(device = %name, rate, channels, ?kind, "microphone opened");
        self.emit(AudioEvent::DeviceActive { name: name.clone(), kind });
        self.open = Some(Open {
            _stream: stream,
            consumer,
            resampler: Resampler::new(rate, RATE as u32),
            broken,
            name,
            using_default,
            last_data: Instant::now(),
        });
        Ok(())
    }

    fn close_stream(&mut self) {
        if self.open.take().is_some() {
            self.ring.clear();
            tracing::info!("microphone closed");
            self.emit(AudioEvent::StreamClosed);
        }
    }

    fn ensure_stream(&mut self) {
        let want = self.should_be_open();
        if !want {
            if self.open.is_some() {
                self.close_stream();
            }
            return;
        }
        // Rebuild broken or silent streams.
        let mut rebuild = false;
        if let Some(o) = &self.open {
            if o.broken.load(Ordering::SeqCst) || o.last_data.elapsed() > Duration::from_secs(2) {
                rebuild = true;
            }
            if o.using_default && self.last_default_check.elapsed() > Duration::from_secs(3) {
                self.last_default_check = Instant::now();
                let cur = cpal::default_host().default_input_device().map(|d| d.to_string());
                if cur.as_deref() != Some(o.name.as_str()) {
                    tracing::info!("default input device changed");
                    rebuild = true;
                }
            }
        }
        if rebuild {
            // Back off so a stream that breaks right after opening cannot spin.
            if self.open.as_ref().is_some_and(|o| o.broken.load(Ordering::SeqCst)) {
                self.reopen_backoff_until = Some(Instant::now() + Duration::from_secs(1));
            }
            // Through close_stream, so the pre-roll ring does not keep the old device's audio.
            self.close_stream();
        }
        if self.open.is_none() {
            if let Some(until) = self.reopen_backoff_until {
                if Instant::now() < until {
                    return;
                }
            }
            if let Err(e) = self.open_stream() {
                tracing::warn!("cannot open microphone: {e}");
                self.emit(AudioEvent::Error(e));
                self.reopen_backoff_until = Some(Instant::now() + Duration::from_secs(2));
            } else {
                self.reopen_backoff_until = None;
            }
        }
    }

    fn pump(&mut self) {
        let Some(o) = self.open.as_mut() else { return };
        let n = o.consumer.slots();
        if n == 0 {
            return;
        }
        let mut raw = Vec::with_capacity(n);
        if let Ok(chunk) = o.consumer.read_chunk(n) {
            raw.extend(chunk);
        }
        o.last_data = Instant::now();
        let samples = o.resampler.process(&raw);
        if samples.is_empty() {
            return;
        }
        // pre-roll ring
        for &s in &samples {
            if self.ring.len() == RATE * RING_SECS {
                self.ring.pop_front();
            }
            self.ring.push_back(s);
        }
        let mut live_speech_now = false;
        if let Some(sess) = self.session.as_mut() {
            let pcm: Vec<i16> = samples.iter().map(|&s| f32_to_i16(s)).collect();
            if let Some(w) = sess.wav.as_mut() {
                if let Err(e) = w.write(&pcm) {
                    tracing::error!("wav write failed: {e}");
                    sess.wav = None;
                }
            }
            let base = sess.pcm.len();
            sess.pcm.extend_from_slice(&pcm);
            if sess.vad.accept(&samples) {
                if sess.speech_start.is_none() {
                    sess.speech_start = Some(base);
                }
                sess.speech_end = Some(sess.pcm.len());
                if !sess.live_speech_reported {
                    sess.live_speech_reported = true;
                    live_speech_now = true;
                }
            }
        }
        if live_speech_now {
            let id = self.session.as_ref().map(|s| s.id).unwrap_or(0);
            self.emit(AudioEvent::Speech { session: id });
        }
        // 40 ms level frames
        let metering = self.session.is_some() || self.monitor;
        for s in samples {
            self.frame_acc.push(s);
            if self.frame_acc.len() == FRAME {
                let frame = std::mem::take(&mut self.frame_acc);
                let (level, db) = self.meter.push(&frame);
                if let Some(sess) = self.session.as_mut() {
                    sess.max_db = sess.max_db.max(db);
                    if !sess.no_signal_reported
                        && sess.started.elapsed() > Duration::from_secs(3)
                        && sess.max_db < -70.0
                    {
                        sess.no_signal_reported = true;
                        let (id, dev) = (sess.id, self.open.as_ref().map(|o| o.name.clone()).unwrap_or_default());
                        self.emit(AudioEvent::NoSignal { session: id, device: dev });
                    }
                }
                if metering {
                    let speech = self.session.as_ref().map(|s| s.speech_end.is_some()).unwrap_or(false);
                    self.emit(AudioEvent::Level { level, db, speech });
                }
                self.frame_acc = Vec::with_capacity(FRAME);
            }
        }
    }

    fn begin(&mut self, id: u64, wav_path: PathBuf) -> Result<(), String> {
        if self.paused {
            return Err("paused".into());
        }
        self.last_used = Some(Instant::now());
        // finalize a dangling session rather than losing it
        if let Some(old) = self.session.take() {
            if let Some(w) = old.wav {
                let _ = w.finalize();
            }
        }
        let mut vad = (self.vad_factory)();
        vad.reset();
        let preroll_n = (self.cfg.preroll_ms as usize * RATE / 1000).min(self.ring.len());
        let pre: Vec<f32> = self.ring.iter().skip(self.ring.len() - preroll_n).copied().collect();
        let mut sess = Session {
            id,
            pcm: Vec::with_capacity(RATE * 30),
            preroll_samples: pre.len(),
            speech_start: None,
            speech_end: None,
            live_speech_reported: false,
            max_db: -100.0,
            wav: match WavWriter::create(&wav_path) {
                Ok(w) => Some(w),
                Err(e) => {
                    tracing::error!("cannot create {}: {e}", wav_path.display());
                    None
                }
            },
            wav_path,
            vad,
            started: Instant::now(),
            no_signal_reported: false,
        };
        if !pre.is_empty() {
            let pcm: Vec<i16> = pre.iter().map(|&s| f32_to_i16(s)).collect();
            if let Some(w) = sess.wav.as_mut() {
                let _ = w.write(&pcm);
            }
            sess.pcm.extend_from_slice(&pcm);
            // Pre-roll speech counts for trimming but not for the tap decision.
            if sess.vad.accept(&pre) {
                sess.speech_start = Some(0);
                sess.speech_end = Some(pcm.len());
            }
            sess.max_db = crate::level::rms_dbfs(&pre).max(-100.0);
        }
        self.session = Some(sess);
        self.ensure_stream();
        if self.open.is_none() {
            // Nothing will be recorded: leave no session behind, and no empty file for
            // crash recovery to find.
            if let Some(s) = self.session.take() {
                drop(s.wav);
                let _ = std::fs::remove_file(&s.wav_path);
            }
            return Err("No microphone found".into());
        }
        Ok(())
    }

    /// A device change requested during a session is applied once it ends.
    fn apply_pending_reopen(&mut self) {
        if self.pending_reopen && self.session.is_none() {
            self.pending_reopen = false;
            self.close_stream();
        }
    }

    fn finish(&mut self, id: u64) -> Option<Take> {
        // drain what is buffered so the last syllable is included
        self.pump();
        let sess = self.session.take()?;
        if sess.id != id {
            self.session = Some(sess);
            return None;
        }
        self.last_used = Some(Instant::now());
        let mut sess = sess;
        if let Some(w) = sess.wav.take() {
            if let Err(e) = w.finalize() {
                tracing::error!("wav finalize failed: {e}");
            }
        }
        let vad_name = sess.vad.name();
        self.apply_pending_reopen();
        Some(Take {
            session: sess.id,
            pcm: sess.pcm,
            preroll_samples: sess.preroll_samples,
            speech_start: sess.speech_start,
            speech_end: sess.speech_end,
            max_db: sess.max_db,
            wav_path: sess.wav_path,
            device: self.open.as_ref().map(|o| o.name.clone()).unwrap_or_default(),
            vad: vad_name,
        })
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        self.ensure_stream();
        loop {
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(cmd) => match cmd {
                    Cmd::Begin { session, wav_path, reply } => {
                        self.pump();
                        let _ = reply.send(self.begin(session, wav_path));
                    }
                    Cmd::Finish { session, reply } => {
                        let _ = reply.send(self.finish(session));
                    }
                    Cmd::Discard { session } => {
                        if self.session.as_ref().map(|s| s.id) == Some(session) {
                            if let Some(s) = self.session.take() {
                                drop(s.wav);
                                let _ = std::fs::remove_file(&s.wav_path);
                            }
                            self.apply_pending_reopen();
                        }
                    }
                    Cmd::Warm => {
                        if self.cfg.mode != MicMode::OnDemand || self.cfg.early_open != MicEarlyOpen::Off {
                            self.last_used = Some(Instant::now());
                        }
                    }
                    Cmd::Cool => {
                        if self.cfg.mode == MicMode::OnDemand
                            && self.cfg.early_open == MicEarlyOpen::CloseOnOtherKey
                            && self.session.is_none()
                        {
                            self.last_used = None;
                        }
                    }
                    Cmd::Monitor(on) => self.monitor = on,
                    Cmd::SetConfig(cfg) => {
                        let reopen = cfg.device != self.cfg.device || cfg.prefer_wired != self.cfg.prefer_wired;
                        self.cfg = cfg;
                        if reopen {
                            if self.session.is_none() {
                                self.close_stream();
                            } else {
                                // Not in the middle of a recording; remembered for afterwards.
                                self.pending_reopen = true;
                            }
                        }
                    }
                    Cmd::SetVadFactory(f) => self.vad_factory = f,
                    Cmd::Pause(p) => {
                        self.paused = p;
                    }
                    Cmd::Shutdown => {
                        if let Some(s) = self.session.take() {
                            if let Some(w) = s.wav {
                                let _ = w.finalize();
                            }
                        }
                        return;
                    }
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.ensure_stream();
            self.pump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(speech: Option<(usize, usize)>, len: usize) -> Take {
        Take {
            session: 1,
            pcm: vec![0; len],
            preroll_samples: 8000,
            speech_start: speech.map(|s| s.0),
            speech_end: speech.map(|s| s.1),
            max_db: -30.0,
            wav_path: PathBuf::new(),
            device: String::new(),
            vad: "silero",
        }
    }

    #[test]
    fn trimming_keeps_margins() {
        let t = take(Some((16_000, 32_000)), 64_000);
        let tr = t.trimmed();
        assert_eq!(tr.len(), 32_000 - 16_000 + 4800 + 6400);
        assert_eq!(t.speech_ms(), 1000);
        assert!(t.has_speech());
    }

    #[test]
    fn no_speech_skips_preroll() {
        let t = take(None, 24_000);
        assert_eq!(t.trimmed().len(), 16_000);
        assert!(!t.has_speech());
    }
}
