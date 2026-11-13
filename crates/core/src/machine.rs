//! Capture state machine: decides tap vs hold, hands-free, command mode and
//! cancellation. Pure: time is passed in, outputs are returned as values.
//!
//! Processing (ASR → cleanup → paste) is a separate sequential job queue in the
//! controller, so a new dictation can start while the previous one is still
//! being processed.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dictation,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    Idle,
    /// Chord down, recording, tap-vs-hold undecided. The pill shows nothing yet.
    Armed { since: Instant, mode: Mode, session: u64 },
    /// Past the tap threshold (or speech seen), still held.
    Holding { since: Instant, mode: Mode, session: u64 },
    HandsFree { since: Instant, session: u64, warned: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    PttDown(Instant),
    PttUp(Instant),
    CommandDown(Instant),
    CommandUp(Instant),
    HandsFreeToggle(Instant),
    /// Bare Space while hands-free (already swallowed by the hook).
    BareSpace(Instant),
    Esc(Instant),
    /// A modifier-only chord was used as part of another shortcut.
    ChordAborted(Instant),
    /// VAD saw speech in the current session.
    Speech(Instant),
    /// The pill's Stop button.
    StopClicked(Instant),
    /// Periodic tick (≈ every 50 ms) for timeouts.
    Tick(Instant),
    /// Stuck-modifier watchdog fired.
    Watchdog(Instant),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Begin a session: open the take (pre-roll + live), start context capture.
    StartCapture { session: u64, mode: Mode },
    /// Hold confirmed: show the listening pill and play the start sound.
    ShowListening { session: u64, mode: Mode, hands_free: bool },
    /// Tap: throw the take away. `replay` asks the hook to re-send the swallowed chord.
    Discard { session: u64, replay: bool },
    /// Stop capture and enqueue the take for processing.
    Finish { session: u64, mode: Mode },
    /// Esc: stop capture; keep audio on disk for 7 days; show "Cancelled · Restore".
    Cancel { session: u64 },
    /// Hook should swallow bare Space (hands-free) or stop doing so.
    SwallowSpace(bool),
    /// Hook should swallow Esc (only while a session is active).
    SwallowEsc(bool),
    /// One minute left in hands-free.
    HandsFreeWarning { session: u64 },
}

#[derive(Debug, Clone)]
pub struct MachineConfig {
    pub tap_max: Duration,
    pub handsfree_max: Duration,
    pub command_max: Duration,
    pub double_tap_handsfree: bool,
    pub double_tap_window: Duration,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            tap_max: Duration::from_millis(250),
            handsfree_max: Duration::from_secs(20 * 60),
            command_max: Duration::from_secs(30),
            double_tap_handsfree: false,
            double_tap_window: Duration::from_millis(400),
        }
    }
}

#[derive(Debug)]
pub struct CaptureMachine {
    pub cfg: MachineConfig,
    state: CaptureState,
    next_session: u64,
    speech_seen: bool,
    /// Release time and mode of the last tap, for double-tap hands-free (dictation only).
    last_tap: Option<(Instant, Mode)>,
}

impl CaptureMachine {
    pub fn new(cfg: MachineConfig) -> Self {
        Self { cfg, state: CaptureState::Idle, next_session: 1, speech_seen: false, last_tap: None }
    }

    pub fn state(&self) -> CaptureState {
        self.state
    }

    pub fn is_capturing(&self) -> bool {
        !matches!(self.state, CaptureState::Idle)
    }

    pub fn current_session(&self) -> Option<u64> {
        match self.state {
            CaptureState::Idle => None,
            CaptureState::Armed { session, .. }
            | CaptureState::Holding { session, .. }
            | CaptureState::HandsFree { session, .. } => Some(session),
        }
    }

    fn new_session(&mut self) -> u64 {
        let s = self.next_session;
        self.next_session += 1;
        self.speech_seen = false;
        s
    }

    fn start(&mut self, t: Instant, mode: Mode) -> Vec<Output> {
        let session = self.new_session();
        self.state = CaptureState::Armed { since: t, mode, session };
        vec![Output::StartCapture { session, mode }, Output::SwallowEsc(true)]
    }

    fn start_handsfree(&mut self, t: Instant, session: Option<u64>) -> Vec<Output> {
        let mut out = Vec::new();
        let session = match session {
            Some(s) => s,
            None => {
                let s = self.new_session();
                out.push(Output::StartCapture { session: s, mode: Mode::Dictation });
                s
            }
        };
        self.state = CaptureState::HandsFree { since: t, session, warned: false };
        out.push(Output::SwallowEsc(true));
        out.push(Output::SwallowSpace(true));
        out.push(Output::ShowListening { session, mode: Mode::Dictation, hands_free: true });
        out
    }

    fn finish(&mut self, session: u64, mode: Mode, was_handsfree: bool) -> Vec<Output> {
        self.state = CaptureState::Idle;
        let mut out = vec![Output::Finish { session, mode }, Output::SwallowEsc(false)];
        if was_handsfree {
            out.push(Output::SwallowSpace(false));
        }
        out
    }

    pub fn step(&mut self, input: Input) -> Vec<Output> {
        use CaptureState::*;
        match (self.state, input) {
            // ---- starting ----
            (Idle, Input::PttDown(t)) => {
                if self.cfg.double_tap_handsfree {
                    if let Some((last, Mode::Dictation)) = self.last_tap {
                        if t.saturating_duration_since(last) <= self.cfg.double_tap_window {
                            self.last_tap = None;
                            return self.start_handsfree(t, None);
                        }
                    }
                }
                self.start(t, Mode::Dictation)
            }
            (Idle, Input::CommandDown(t)) => self.start(t, Mode::Command),
            (Idle, Input::HandsFreeToggle(t)) => self.start_handsfree(t, None),

            // ---- armed: tap vs hold ----
            (Armed { since, mode, session }, Input::Speech(_)) => {
                self.speech_seen = true;
                let _ = since;
                self.state = Holding { since, mode, session };
                vec![Output::ShowListening { session, mode, hands_free: false }]
            }
            (Armed { since, mode, session }, Input::Tick(t)) => {
                if t.saturating_duration_since(since) >= self.cfg.tap_max {
                    self.state = Holding { since, mode, session };
                    vec![Output::ShowListening { session, mode, hands_free: false }]
                } else {
                    vec![]
                }
            }
            (Armed { since, mode, session }, Input::PttUp(t) | Input::CommandUp(t)) => {
                let held = t.saturating_duration_since(since);
                if held < self.cfg.tap_max && !self.speech_seen {
                    self.state = Idle;
                    self.last_tap = Some((t, mode));
                    vec![Output::Discard { session, replay: true }, Output::SwallowEsc(false)]
                } else {
                    self.finish(session, mode, false)
                }
            }
            (Armed { session, .. }, Input::ChordAborted(_)) => {
                self.state = Idle;
                vec![Output::Discard { session, replay: false }, Output::SwallowEsc(false)]
            }
            // Hands-free chord right after the hold began (before the first tick): keep listening.
            (Armed { mode: Mode::Dictation, session, .. }, Input::HandsFreeToggle(t)) => self.start_handsfree(t, Some(session)),
            // Esc or a stuck-key watchdog before anything was said or shown: nothing to keep.
            (Armed { session, .. }, Input::Esc(_) | Input::Watchdog(_)) if !self.speech_seen => {
                self.state = Idle;
                vec![Output::Discard { session, replay: false }, Output::SwallowEsc(false)]
            }
            (Armed { session, .. } | Holding { session, .. }, Input::Esc(_)) => {
                self.state = Idle;
                vec![Output::Cancel { session }, Output::SwallowEsc(false)]
            }
            // The pill's Stop button during a hold ends it like a key-up.
            (Armed { session, mode, .. } | Holding { session, mode, .. }, Input::StopClicked(_)) => self.finish(session, mode, false),

            // ---- holding ----
            (Holding { mode, session, .. }, Input::PttUp(_) | Input::CommandUp(_)) => {
                self.finish(session, mode, false)
            }
            (Holding { since, mode: Mode::Command, session }, Input::Tick(t)) => {
                if t.saturating_duration_since(since) >= self.cfg.command_max {
                    self.finish(session, Mode::Command, false)
                } else {
                    vec![]
                }
            }
            // Hands-free chord while holding PTT: keep listening after release.
            (Holding { mode: Mode::Dictation, session, .. }, Input::HandsFreeToggle(t)) => {
                self.start_handsfree(t, Some(session))
            }

            // ---- hands-free ----
            (
                HandsFree { session, .. },
                Input::BareSpace(_) | Input::HandsFreeToggle(_) | Input::PttDown(_) | Input::StopClicked(_),
            ) => self.finish(session, Mode::Dictation, true),
            (HandsFree { session, .. }, Input::Esc(_)) => {
                self.state = Idle;
                vec![Output::Cancel { session }, Output::SwallowEsc(false), Output::SwallowSpace(false)]
            }
            (HandsFree { since, session, warned }, Input::Tick(t)) => {
                let elapsed = t.saturating_duration_since(since);
                if elapsed >= self.cfg.handsfree_max {
                    self.finish(session, Mode::Dictation, true)
                } else if !warned
                    && elapsed + Duration::from_secs(60) >= self.cfg.handsfree_max
                {
                    self.state = HandsFree { since, session, warned: true };
                    vec![Output::HandsFreeWarning { session }]
                } else {
                    vec![]
                }
            }
            (HandsFree { .. }, Input::Speech(_)) => {
                self.speech_seen = true;
                vec![]
            }
            (Holding { .. }, Input::Speech(_)) => {
                self.speech_seen = true;
                vec![]
            }

            // ---- watchdog: a key looks stuck; finish what we have, never lose audio ----
            (Armed { session, mode, .. } | Holding { session, mode, .. }, Input::Watchdog(_)) => {
                self.finish(session, mode, false)
            }
            (HandsFree { session, .. }, Input::Watchdog(_)) => {
                self.finish(session, Mode::Dictation, true)
            }

            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> (CaptureMachine, Instant) {
        (CaptureMachine::new(MachineConfig::default()), Instant::now())
    }
    fn ms(t: Instant, n: u64) -> Instant {
        t + Duration::from_millis(n)
    }

    #[test]
    fn quick_tap_is_discarded_and_replayed() {
        let (mut m, t) = m();
        let out = m.step(Input::PttDown(t));
        assert!(matches!(out[0], Output::StartCapture { mode: Mode::Dictation, .. }));
        assert!(m.step(Input::Tick(ms(t, 100))).is_empty());
        let out = m.step(Input::PttUp(ms(t, 120)));
        assert!(matches!(out[0], Output::Discard { replay: true, .. }));
        assert_eq!(m.state(), CaptureState::Idle);
    }

    #[test]
    fn hold_past_threshold_shows_listening_then_finishes() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        let out = m.step(Input::Tick(ms(t, 260)));
        assert!(matches!(out[0], Output::ShowListening { hands_free: false, .. }));
        let out = m.step(Input::PttUp(ms(t, 3000)));
        assert!(matches!(out[0], Output::Finish { mode: Mode::Dictation, .. }));
    }

    #[test]
    fn speech_wins_over_short_hold() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        let out = m.step(Input::Speech(ms(t, 80)));
        assert!(matches!(out[0], Output::ShowListening { .. }));
        let out = m.step(Input::PttUp(ms(t, 200)));
        assert!(matches!(out[0], Output::Finish { .. }));
    }

    #[test]
    fn release_before_tick_but_after_threshold_finishes() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        let out = m.step(Input::PttUp(ms(t, 400)));
        assert!(matches!(out[0], Output::Finish { .. }));
    }

    #[test]
    fn esc_cancels_hold() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        m.step(Input::Tick(ms(t, 300)));
        let out = m.step(Input::Esc(ms(t, 500)));
        assert!(matches!(out[0], Output::Cancel { .. }));
        assert_eq!(m.state(), CaptureState::Idle);
        // the key-up that follows is ignored
        assert!(m.step(Input::PttUp(ms(t, 600))).is_empty());
    }

    #[test]
    fn handsfree_space_stops_and_releases_swallow() {
        let (mut m, t) = m();
        let out = m.step(Input::HandsFreeToggle(t));
        assert!(out.contains(&Output::SwallowSpace(true)));
        assert!(out.iter().any(|o| matches!(o, Output::ShowListening { hands_free: true, .. })));
        let out = m.step(Input::BareSpace(ms(t, 5000)));
        assert!(matches!(out[0], Output::Finish { .. }));
        assert!(out.contains(&Output::SwallowSpace(false)));
    }

    #[test]
    fn handsfree_esc_cancels() {
        let (mut m, t) = m();
        m.step(Input::HandsFreeToggle(t));
        let out = m.step(Input::Esc(ms(t, 1000)));
        assert!(matches!(out[0], Output::Cancel { .. }));
        assert!(out.contains(&Output::SwallowSpace(false)));
    }

    #[test]
    fn handsfree_warns_then_autofinishes() {
        let (mut m, t) = m();
        m.step(Input::HandsFreeToggle(t));
        let out = m.step(Input::Tick(t + Duration::from_secs(19 * 60 + 1)));
        assert!(matches!(out[0], Output::HandsFreeWarning { .. }));
        assert!(m.step(Input::Tick(t + Duration::from_secs(19 * 60 + 2))).is_empty());
        let out = m.step(Input::Tick(t + Duration::from_secs(20 * 60)));
        assert!(matches!(out[0], Output::Finish { .. }));
    }

    #[test]
    fn command_mode_auto_commits_at_limit() {
        let (mut m, t) = m();
        let out = m.step(Input::CommandDown(t));
        assert!(matches!(out[0], Output::StartCapture { mode: Mode::Command, .. }));
        m.step(Input::Tick(ms(t, 300)));
        let out = m.step(Input::Tick(t + Duration::from_secs(30)));
        assert!(matches!(out[0], Output::Finish { mode: Mode::Command, .. }));
    }

    #[test]
    fn hold_then_handsfree_chord_continues_session() {
        let (mut m, t) = m();
        let out = m.step(Input::PttDown(t));
        let Output::StartCapture { session, .. } = out[0] else { panic!() };
        m.step(Input::Tick(ms(t, 300)));
        let out = m.step(Input::HandsFreeToggle(ms(t, 1000)));
        assert!(!out.iter().any(|o| matches!(o, Output::StartCapture { .. })));
        assert_eq!(m.current_session(), Some(session));
        // PTT release no longer stops it
        assert!(m.step(Input::PttUp(ms(t, 1100))).is_empty());
    }

    #[test]
    fn double_tap_option() {
        let mut m = CaptureMachine::new(MachineConfig { double_tap_handsfree: true, ..Default::default() });
        let t = Instant::now();
        m.step(Input::PttDown(t));
        m.step(Input::PttUp(ms(t, 80)));
        let out = m.step(Input::PttDown(ms(t, 250)));
        assert!(out.contains(&Output::SwallowSpace(true)));
    }

    #[test]
    fn aborted_modifier_chord_discards_without_replay() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        let out = m.step(Input::ChordAborted(ms(t, 50)));
        assert!(matches!(out[0], Output::Discard { replay: false, .. }));
    }

    #[test]
    fn watchdog_finishes_instead_of_losing_audio() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        m.step(Input::Tick(ms(t, 300)));
        let out = m.step(Input::Watchdog(ms(t, 9000)));
        assert!(matches!(out[0], Output::Finish { .. }));
    }

    #[test]
    fn handsfree_chord_while_armed_keeps_the_session() {
        let (mut m, t) = m();
        let Output::StartCapture { session, .. } = m.step(Input::PttDown(t))[0] else { panic!() };
        let out = m.step(Input::HandsFreeToggle(ms(t, 100)));
        assert!(out.iter().any(|o| matches!(o, Output::ShowListening { hands_free: true, .. })));
        assert_eq!(m.current_session(), Some(session));
        assert!(m.step(Input::PttUp(ms(t, 150))).is_empty(), "the release no longer stops it");
    }

    #[test]
    fn stop_button_ends_a_hold() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        m.step(Input::Tick(ms(t, 300)));
        let out = m.step(Input::StopClicked(ms(t, 2000)));
        assert!(matches!(out[0], Output::Finish { .. }));
    }

    #[test]
    fn esc_before_speech_discards_quietly() {
        let (mut m, t) = m();
        m.step(Input::PttDown(t));
        let out = m.step(Input::Esc(ms(t, 100)));
        assert!(matches!(out[0], Output::Discard { replay: false, .. }), "{out:?}");
        // with speech it is a real cancel that keeps the audio
        let mut spoken = CaptureMachine::new(MachineConfig::default());
        spoken.step(Input::PttDown(t));
        spoken.step(Input::Speech(ms(t, 50)));
        let out = spoken.step(Input::Esc(ms(t, 100)));
        assert!(matches!(out[0], Output::Cancel { .. }));
    }

    #[test]
    fn a_command_tap_does_not_arm_double_tap() {
        let mut m = CaptureMachine::new(MachineConfig { double_tap_handsfree: true, ..Default::default() });
        let t = Instant::now();
        m.step(Input::CommandDown(t));
        m.step(Input::CommandUp(ms(t, 80)));
        let out = m.step(Input::PttDown(ms(t, 250)));
        assert!(!out.contains(&Output::SwallowSpace(true)));
        assert!(matches!(out[0], Output::StartCapture { mode: Mode::Dictation, .. }));
    }

    #[test]
    fn sessions_are_unique() {
        let (mut m, t) = m();
        let a = m.step(Input::PttDown(t));
        m.step(Input::PttUp(ms(t, 500)));
        let b = m.step(Input::PttDown(ms(t, 600)));
        assert_ne!(a[0], b[0]);
    }
}
