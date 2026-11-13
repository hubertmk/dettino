//! Controller thread: hotkey and audio events in, capture-machine outputs out.
//! Processing happens in the pipeline task so a new dictation can start while
//! the previous one is still being transcribed.

use crate::app::{app, now_ms};
use crate::pill::{self, PillAction, PillState};
use crate::pipeline::{self, Job};
use app_audio::AudioEvent;
use app_core::machine::MachineConfig;
use app_core::{CaptureMachine, Input, Mode, Output};
use app_hotkey::{ChordKind, HookEvent, KeyDiag};
use app_platform::{ContextRequest, ContextSnapshot, ForegroundInfo, SoundKind};
use crossbeam_channel::{select, Receiver};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub enum CtlMsg {
    /// The user clicked an action on the pill.
    PillAction(String),
    /// Settings changed: rebuild machine config.
    Reload,
}

pub struct SessionInfo {
    pub uid: String,
    pub mode: Mode,
    pub hands_free: bool,
    pub target: Option<ForegroundInfo>,
    pub context: Arc<Mutex<Option<ContextSnapshot>>>,
    pub started: Instant,
    pub start_error: Option<String>,
}

fn machine_config() -> MachineConfig {
    let s = app().settings();
    MachineConfig {
        tap_max: Duration::from_millis(s.tap_max_ms.clamp(120, 600)),
        handsfree_max: Duration::from_secs(s.handsfree_max_min.clamp(1, 120) * 60),
        command_max: Duration::from_secs(30),
        double_tap_handsfree: s.double_tap_handsfree,
        double_tap_window: Duration::from_millis(400),
    }
}

pub fn sound(kind: SoundKind) {
    let s = app().settings();
    if s.sounds {
        app_platform::play_sound(kind, s.sound_style, s.sound_volume);
    }
}

/// Is dictation blocked in the foreground app (denylist), unless temporarily allowed?
fn blocked_app(fg: &Option<ForegroundInfo>) -> Option<String> {
    let fg = fg.as_ref()?;
    if fg.is_self {
        return None;
    }
    let a = app();
    let s = a.settings.read().unwrap();
    let rule = s.rule_for(&fg.app)?;
    if !rule.never_dictate {
        return None;
    }
    if let Some(until) = a.allow_until.lock().unwrap().get(&fg.app) {
        if Instant::now() < *until {
            return None;
        }
    }
    Some(fg.app.clone())
}

/// Whether `failures` microphone opens in a row deserve the "No microphone" badge. At
/// login the audio service can still be coming up; the engine retries after two
/// seconds, so one failure is not yet news.
fn mic_badge_after(failures: u32) -> bool {
    failures >= 2
}

pub fn run(hk: Receiver<(HookEvent, Instant)>, mut audio: Receiver<AudioEvent>, ctl: Receiver<CtlMsg>) {
    let mut machine = CaptureMachine::new(machine_config());
    let mut sessions: HashMap<u64, SessionInfo> = HashMap::new();
    let mut mic_failures = 0u32;
    let mut last_watchdog = Instant::now();
    let mut last_level = Instant::now();
    let tick = crossbeam_channel::tick(Duration::from_millis(50));

    loop {
        let mut inputs: Vec<Input> = Vec::new();
        select! {
            recv(hk) -> msg => {
                let Ok((ev, t)) = msg else { return };
                match ev {
                    HookEvent::ModifierDown | HookEvent::ModifierCancel => {}
                    HookEvent::KeyDiag(d) => log_key_diag(d),
                    HookEvent::HookAlive => tracing::info!("keyboard hook is receiving events"),
                    _ => tracing::info!("hotkey: {ev:?}"),
                }
                handle_hook(ev, t, &machine, &mut inputs);
            }
            recv(audio) -> msg => {
                let Ok(ev) = msg else {
                    // The audio engine is gone. Shortcuts and the pill keep working, and the
                    // badge says why nothing is recorded.
                    tracing::error!("audio engine stopped; restart Dettino");
                    pill::set_badge(Some("no_mic"), Some("Audio stopped — restart Dettino"));
                    audio = crossbeam_channel::never();
                    continue;
                };
                match ev {
                    AudioEvent::Speech { session } => {
                        if machine.current_session() == Some(session) {
                            inputs.push(Input::Speech(Instant::now()));
                        }
                    }
                    AudioEvent::Level { level, speech, .. } => {
                        if machine.is_capturing() && last_level.elapsed() >= Duration::from_millis(30) {
                            last_level = Instant::now();
                            pill::send_level(level, speech);
                        }
                        crate::commands::send_mic_level(level, speech);
                    }
                    AudioEvent::NoSignal { session, device } => {
                        if machine.current_session() == Some(session) {
                            let mut s = pill::current();
                            if s.kind == "listening" {
                                s.quiet = true;
                                s.label = Some(format!("Can't hear you — check {}", short_device(&device)));
                                pill::set(s);
                            }
                        }
                    }
                    AudioEvent::Error(e) => {
                        tracing::warn!("audio: {e}");
                        mic_failures = mic_failures.saturating_add(1);
                        if mic_badge_after(mic_failures) {
                            pill::set_badge(Some("no_mic"), Some("No microphone"));
                        }
                    }
                    AudioEvent::DeviceActive { .. } => {
                        mic_failures = 0;
                        if pill::current().badge.as_deref() == Some("no_mic") {
                            pill::set_badge(None, None);
                        }
                    }
                    AudioEvent::StreamClosed => {}
                }
            }
            recv(ctl) -> msg => {
                let Ok(m) = msg else { return };
                match m {
                    CtlMsg::Reload => machine.cfg = machine_config(),
                    CtlMsg::PillAction(id) => {
                        if id == "stop" {
                            inputs.push(Input::StopClicked(Instant::now()));
                        } else {
                            pipeline::pill_action(&id);
                        }
                    }
                }
            }
            recv(tick) -> _ => {
                let now = Instant::now();
                inputs.push(Input::Tick(now));
                crate::commands::resume_hotkeys_if_stale();
                // A held chord whose key-up got lost must not keep a session open forever:
                // past the hands-free limit, finish what was recorded.
                if let Some(info) = machine.current_session().and_then(|id| sessions.get(&id)) {
                    if info.started.elapsed() > machine.cfg.handsfree_max + Duration::from_secs(30) {
                        tracing::warn!("session open for too long; finishing it");
                        inputs.push(Input::Watchdog(now));
                    }
                }
                // Always, not only while capturing: a modifier whose key-up another hook
                // ate must be released before the next chord, not during it.
                if last_watchdog.elapsed() > Duration::from_millis(500) {
                    last_watchdog = Instant::now();
                    for ev in app_hotkey::resync() {
                        tracing::warn!("watchdog: key state resynced ({ev:?})");
                        handle_hook(ev, Instant::now(), &machine, &mut inputs);
                    }
                }
            }
        }
        for input in inputs {
            for out in machine.step(input) {
                tracing::info!("capture: {out:?}");
                handle_output(out, &mut sessions);
            }
        }
    }
}

/// One bound key-down as the hook saw it, with the window it went to. This is the
/// line to read when "the shortcut does nothing": it shows whether the key reached
/// the hook at all, which modifiers the hook and the OS each thought were held,
/// and what the tracker decided.
fn log_key_diag(d: KeyDiag) {
    let fg = app_platform::foreground();
    let (app, elevated, is_self) = fg.as_ref().map(|f| (f.app.as_str(), f.blocked_elevated, f.is_self)).unwrap_or(("", false, false));
    tracing::info!(
        key = app_core::keys::key_name(d.vk).as_str(),
        vk = d.vk,
        flags = d.flags,
        tracked_mods = d.tracked_mods,
        os_mods = d.os_mods,
        outcome = ?d.outcome,
        app,
        elevated,
        is_self,
        "hotkey diag"
    );
}

pub(crate) fn short_device(d: &str) -> String {
    d.split('(').next().unwrap_or(d).trim().chars().take(28).collect()
}

fn handle_hook(ev: HookEvent, t: Instant, machine: &CaptureMachine, inputs: &mut Vec<Input>) {
    let starting = |kind: ChordKind, inputs: &mut Vec<Input>, input: Input| {
        if machine.is_capturing() {
            inputs.push(input);
            return;
        }
        let a = app();
        if a.settings.read().unwrap().paused {
            app_hotkey::replay(kind);
            pill::flash(PillState { kind: "notice".into(), label: Some("Paused — click the pill to resume".into()), ..Default::default() }, 1800);
            return;
        }
        let fg = app_platform::foreground();
        if fg.as_ref().is_some_and(|f| f.is_self) {
            // Our own window: a dictation could only ever be pasted into Dettino.
            app_hotkey::replay(kind);
            return;
        }
        if let Some(blocked) = blocked_app(&fg) {
            app_hotkey::replay(kind);
            pill::place(fg.as_ref());
            pill::flash(
                PillState {
                    kind: "blocked".into(),
                    label: Some(format!("Off in {}", fg.as_ref().map(|f| f.display_name()).unwrap_or_else(|| app_platform::display_app_name(&blocked)))),
                    action: Some(PillAction { id: format!("allow:{blocked}"), label: "Allow 5 min".into() }),
                    ..Default::default()
                },
                3500,
            );
            return;
        }
        inputs.push(input);
    };
    match ev {
        HookEvent::ChordDown(ChordKind::PushToTalk) => starting(ChordKind::PushToTalk, inputs, Input::PttDown(t)),
        HookEvent::ChordUp(ChordKind::PushToTalk) => inputs.push(Input::PttUp(t)),
        HookEvent::ChordDown(ChordKind::Command) => starting(ChordKind::Command, inputs, Input::CommandDown(t)),
        HookEvent::ChordUp(ChordKind::Command) => inputs.push(Input::CommandUp(t)),
        HookEvent::ChordDown(ChordKind::HandsFree) => starting(ChordKind::HandsFree, inputs, Input::HandsFreeToggle(t)),
        HookEvent::ChordUp(ChordKind::HandsFree) => {}
        HookEvent::BareSpace => inputs.push(Input::BareSpace(t)),
        HookEvent::Esc => inputs.push(Input::Esc(t)),
        HookEvent::ChordAborted(_) => inputs.push(Input::ChordAborted(t)),
        HookEvent::SpaceSwallowCapHit => inputs.push(Input::StopClicked(t)),
        HookEvent::ModifierDown => app().audio.warm(),
        HookEvent::ModifierCancel => app().audio.cool(),
        HookEvent::ChordDown(ChordKind::UndoAi) => {
            app().audio.cool();
            std::thread::spawn(pipeline::undo_ai);
        }
        HookEvent::ChordDown(ChordKind::PasteLast) => {
            app().audio.cool();
            std::thread::spawn(pipeline::paste_last);
        }
        HookEvent::ChordUp(_) => {}
        HookEvent::KeyDiag(_) | HookEvent::HookAlive => {}
    }
}

fn start_context(info: &SessionInfo) {
    let a = app();
    let s = a.settings();
    let rule_ctx = info
        .target
        .as_ref()
        .and_then(|t| s.rule_for(&t.app))
        .and_then(|r| r.context)
        .unwrap_or(true);
    let on = s.context.enabled && rule_ctx;
    let req = ContextRequest {
        caret_text: on && s.context.caret_text,
        selection: (on && s.context.selection) || info.mode == Mode::Command,
        visible_text: on && s.context.visible_text,
    };
    let slot = info.context.clone();
    let budget = if req.visible_text { 250 } else { 150 };
    std::thread::spawn(move || {
        let snap = app_platform::capture_context(req, Duration::from_millis(budget));
        *slot.lock().unwrap() = Some(snap);
    });
}

fn handle_output(out: Output, sessions: &mut HashMap<u64, SessionInfo>) {
    let a = app();
    match out {
        Output::StartCapture { session, mode } => {
            let target = app_platform::foreground().filter(|f| !f.is_self);
            pill::place(target.as_ref());
            let uid = app_storage::History::new_id();
            let wav = a.paths.sessions().join(format!("{uid}.wav"));
            let start_error = a.audio.begin(session, wav).err();
            if let Some(e) = &start_error {
                tracing::warn!("cannot start capture: {e}");
            }
            let info = SessionInfo {
                uid,
                mode,
                hands_free: false,
                target,
                context: Arc::new(Mutex::new(None)),
                started: Instant::now(),
                start_error,
            };
            start_context(&info);
            sessions.insert(session, info);
            pill::set_capturing(true);
        }
        Output::ShowListening { session, mode, hands_free } => {
            if let Some(info) = sessions.get_mut(&session) {
                info.hands_free = hands_free;
                if let Some(e) = info.start_error.clone() {
                    sound(SoundKind::Error);
                    pill::flash(
                        PillState {
                            kind: "error".into(),
                            label: Some(if e.contains("No microphone") { "No microphone found".into() } else { "Microphone unavailable".into() }),
                            action: Some(PillAction { id: "open:audio".into(), label: "Open Audio".into() }),
                            ..Default::default()
                        },
                        6000,
                    );
                    return;
                }
            }
            sound(if hands_free { SoundKind::HandsFree } else { SoundKind::Start });
            let mut s = PillState {
                kind: "listening".into(),
                mode: if mode == Mode::Command { "command".into() } else { "dictation".into() },
                hands_free,
                ..Default::default()
            };
            if hands_free {
                s.label = Some("Space to finish · Esc to cancel".into());
                s.action = Some(PillAction { id: "stop".into(), label: "Stop".into() });
            }
            if mode == Mode::Command {
                s.label = Some("Say how to change the selection".into());
            }
            pill::set(s);
        }
        Output::Discard { session, replay } => {
            pill::set_capturing(false);
            let info = sessions.remove(&session);
            a.audio.discard(session);
            if replay {
                let kind = match info.map(|i| i.mode) {
                    Some(Mode::Command) => ChordKind::Command,
                    _ => ChordKind::PushToTalk,
                };
                app_hotkey::replay(kind);
            }
            if pill::current().kind == "listening" {
                pill::set(pill::state("idle"));
            }
        }
        Output::Finish { session, mode } => {
            pill::set_capturing(false);
            let Some(info) = sessions.remove(&session) else { return };
            let take = a.audio.finish(session);
            if info.start_error.is_some() {
                return;
            }
            sound(SoundKind::Stop);
            let hold_ms = info.started.elapsed().as_millis() as u64;
            pill::set(PillState { kind: "thinking".into(), mode: if mode == Mode::Command { "command".into() } else { "dictation".into() }, ..Default::default() });
            let job = Job {
                uid: info.uid,
                mode,
                take,
                target: info.target,
                context: info.context,
                hold_ms,
                key_up: Instant::now(),
                from_history: None,
            };
            pipeline::enqueue(job);
        }
        Output::Cancel { session } => {
            pill::set_capturing(false);
            let Some(info) = sessions.remove(&session) else { return };
            let take = a.audio.finish(session);
            let kept = pipeline::store_cancelled(&info, take);
            // Restore only when the recording was kept (history on).
            let action = kept.then(|| PillAction { id: "restore".into(), label: "Restore".into() });
            pill::flash(PillState { kind: "cancelled".into(), label: Some("Cancelled".into()), action, ..Default::default() }, 4000);
        }
        Output::SwallowSpace(on) => app_hotkey::set_swallow_space(on),
        Output::SwallowEsc(on) => app_hotkey::set_swallow_esc(on),
        Output::HandsFreeWarning { .. } => {
            let mut s = pill::current();
            s.label = Some("1 minute left — press Space to finish".into());
            pill::set(s);
        }
    }
    let _ = now_ms;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_failed_microphone_open_at_launch_is_not_yet_a_badge() {
        assert!(!mic_badge_after(1), "the audio service may still be coming up");
        assert!(mic_badge_after(2));
        assert!(mic_badge_after(7));
    }
}
