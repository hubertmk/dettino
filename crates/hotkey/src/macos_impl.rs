//! macOS backend: an active `CGEventTap` on a dedicated thread with its own
//! `CFRunLoop`, plus `CGEventPost`-based injection tagged with [`INJECT_MARKER`]
//! in `kCGEventSourceUserData`.
//!
//! An active (swallowing) tap needs the Accessibility permission. Until it is
//! granted, tap creation fails; the thread retries every second, so shortcuts start
//! working as soon as the user flips the switch in System Settings, without a restart.
//!
//! Keycodes are mapped to Windows VKs ([`crate::mac_keys`]) so the tracker, the
//! settings and the UI are shared. Modifiers arrive as `FlagsChanged` events and
//! never autorepeat; their side is decoded from the device-dependent flag bits.

use crate::mac_keys::{self, flags_for_mods, from_vk, mods_from_flags, side_mask, to_vk};
use crate::tracker::{ChordKind, ChordTracker, DiagOutcome, HookEvent, KeyDiag, KeyEvent};
use crate::INJECT_MARKER;
use app_core::keys::{is_modifier, modifier_bit, vk, Hotkeys};
use crossbeam_channel::{unbounded, Receiver, Sender};
use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use std::ffi::c_void;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static TRACKER: Mutex<Option<ChordTracker>> = Mutex::new(None);
static SENDER: OnceLock<Sender<(HookEvent, Instant)>> = OnceLock::new();
/// The tap's mach port, for re-enabling it after the system disabled it.
static TAP: AtomicPtr<CFMachPort> = AtomicPtr::new(null_mut());
static RUN_LOOP: Mutex<Option<RunLoop>> = Mutex::new(None);
static STOPPED: AtomicBool = AtomicBool::new(false);
/// Counts real (non-injected, non-modifier, not swallowed) key-downs: did the user type since a paste?
static USER_KEYS: AtomicU64 = AtomicU64::new(0);
/// First event seen (reported once as `HookEvent::HookAlive`).
static HOOK_ALIVE: AtomicBool = AtomicBool::new(false);
/// When set, every key-down produces a `KeyDiag`, not only bound keys.
static TRACE: AtomicBool = AtomicBool::new(false);

/// `CFRunLoopStop` may be called from any thread.
struct RunLoop(CFRetained<CFRunLoop>);
unsafe impl Send for RunLoop {}

/// Modifier VKs whose state the tracker reconciles, with their device-dependent flag bit.
const SIDE_KEYS: [u16; 8] = [vk::LCONTROL, vk::RCONTROL, vk::LSHIFT, vk::RSHIFT, vk::LMENU, vk::RMENU, vk::LWIN, vk::RWIN];

/// Is `code` down according to a set of event flags?
fn down_in_flags(flags: u64, code: u16) -> Option<bool> {
    let any = |a: u16, b: u16| flags & (side_mask(a) | side_mask(b)) != 0;
    Some(match code {
        vk::CONTROL => any(vk::LCONTROL, vk::RCONTROL),
        vk::SHIFT => any(vk::LSHIFT, vk::RSHIFT),
        vk::MENU => any(vk::LMENU, vk::RMENU),
        c if SIDE_KEYS.contains(&c) || c == vk::FN => flags & side_mask(c) != 0,
        _ => return None,
    })
}

fn hid_flags() -> u64 {
    CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState).0
}

fn os_key_down(code: u16) -> bool {
    if let Some(d) = down_in_flags(hid_flags(), code) {
        return d;
    }
    from_vk(code).is_some_and(|k| CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, k))
}

fn reenable_tap() {
    let tap = TAP.load(Ordering::SeqCst);
    if let Some(tap) = unsafe { tap.as_ref() } {
        CGEvent::tap_enable(tap, true);
    }
    // Key state is unknown after a gap; a chord that was held must end, or the
    // session never does.
    let dropped = TRACKER.lock().ok().and_then(|mut g| g.as_mut().map(|t| t.reset())).unwrap_or_default();
    if let Some(tx) = SENDER.get() {
        let now = Instant::now();
        for ev in dropped {
            let _ = tx.send((ev, now));
        }
    }
}

unsafe extern "C-unwind" fn tap_proc(_proxy: CGEventTapProxy, ty: CGEventType, event: NonNull<CGEvent>, _info: *mut c_void) -> *mut CGEvent {
    let pass = event.as_ptr();
    if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
        tracing::warn!("event tap was disabled ({}); re-enabling", ty.0);
        reenable_tap();
        return pass;
    }
    let ev = Some(event.as_ref());
    if CGEvent::integer_value_field(ev, CGEventField::EventSourceUserData) == INJECT_MARKER as i64 {
        return pass;
    }
    let keycode = CGEvent::integer_value_field(ev, CGEventField::KeyboardEventKeycode) as u16;
    let flags = CGEvent::flags(ev).0;
    let code = to_vk(keycode);
    let down = match ty {
        CGEventType::KeyDown => true,
        CGEventType::KeyUp => false,
        CGEventType::FlagsChanged => match down_in_flags(flags, code) {
            Some(d) => d,
            // Caps Lock reports the lock state, not the key: each change is one press.
            None if code == vk::CAPITAL => {
                let _ = handle(code, true, flags);
                false
            }
            None => return pass,
        },
        _ => return pass,
    };
    if code == 0 {
        if down {
            USER_KEYS.fetch_add(1, Ordering::Relaxed);
        }
        return pass;
    }
    if handle(code, down, flags) {
        null_mut()
    } else {
        pass
    }
}

/// Run one key event through the tracker. Returns true to swallow it.
fn handle(code: u16, down: bool, flags: u64) -> bool {
    let now = Instant::now();
    let tx = SENDER.get();
    if !HOOK_ALIVE.swap(true, Ordering::Relaxed) {
        if let Some(tx) = tx {
            let _ = tx.send((HookEvent::HookAlive, now));
        }
    }
    let trace = TRACE.load(Ordering::Relaxed);
    let mut diag = None;
    // The event's own flags are the most current modifier state there is.
    let is_down = |c: u16| down_in_flags(flags, c).unwrap_or_else(|| os_key_down(c));
    let decision = match TRACKER.lock() {
        Ok(mut guard) => guard.as_mut().map(|t| {
            let diagworthy = down && (t.is_bound_key(code) || trace);
            let repeat = t.is_held(code);
            let masks = if diagworthy { Some(t.reconcile_modifiers(code, now, is_down)) } else { None };
            let d = t.on_key(KeyEvent { vk: code, down }, now);
            if let (Some((tracked, os)), false) = (masks, repeat) {
                let outcome = match d.event {
                    Some(HookEvent::ChordDown(kind)) => DiagOutcome::Matched(kind),
                    _ if d.swallow => DiagOutcome::Swallowed,
                    _ if (tracked | os) != 0 => DiagOutcome::NoMatch,
                    _ => DiagOutcome::Passed,
                };
                diag = Some(KeyDiag { vk: code, flags: flags as u32, tracked_mods: tracked, os_mods: os, outcome });
            }
            d
        }),
        Err(_) => None,
    };
    if down && !decision.map(|d| d.swallow).unwrap_or(false) && !is_modifier(code) {
        USER_KEYS.fetch_add(1, Ordering::Relaxed);
    }
    if let Some(tx) = tx {
        if let Some(d) = diag {
            let _ = tx.send((HookEvent::KeyDiag(d), now));
        }
        if let Some(ev) = decision.and_then(|d| d.event) {
            let _ = tx.send((ev, now));
        }
    }
    decision.is_some_and(|d| d.swallow)
}

fn event_mask() -> CGEventMask {
    [CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged].iter().fold(0, |m, t| m | (1u64 << t.0))
}

/// Create the tap and run its run loop until [`stop`]. Returns false if the tap
/// could not be created (no Accessibility permission yet).
fn run_tap() -> bool {
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            event_mask(),
            Some(tap_proc),
            null_mut(),
        )
    };
    let Some(tap) = tap else { return false };
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else { return false };
    let Some(rl) = CFRunLoop::current() else { return false };
    rl.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    TAP.store(CFRetained::as_ptr(&tap).as_ptr(), Ordering::SeqCst);
    CGEvent::tap_enable(&tap, true);
    *RUN_LOOP.lock().unwrap() = Some(RunLoop(rl));
    tracing::info!("event tap installed");
    CFRunLoop::run();
    TAP.store(null_mut(), Ordering::SeqCst);
    drop(tap);
    true
}

/// Start the tap thread. Events arrive on the returned receiver.
pub fn start(hotkeys: &Hotkeys) -> Result<Receiver<(HookEvent, Instant)>, String> {
    let (tx, rx) = unbounded();
    SENDER.set(tx).map_err(|_| "hotkey hook already started".to_string())?;
    *TRACKER.lock().unwrap() = Some(ChordTracker::new(hotkeys));
    std::thread::Builder::new()
        .name("hotkey-tap".into())
        .spawn(|| {
            let mut warned = false;
            while !STOPPED.load(Ordering::SeqCst) {
                if run_tap() {
                    return;
                }
                if !warned {
                    warned = true;
                    tracing::warn!("event tap unavailable; waiting for the Accessibility permission");
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(rx)
}

/// True once the event tap is installed (Accessibility granted).
pub fn is_active() -> bool {
    !TAP.load(Ordering::SeqCst).is_null()
}

pub fn stop() {
    STOPPED.store(true, Ordering::SeqCst);
    if let Some(rl) = RUN_LOOP.lock().unwrap().take() {
        rl.0.stop();
    }
}

fn with_tracker<R>(f: impl FnOnce(&mut ChordTracker) -> R) -> Option<R> {
    TRACKER.lock().ok()?.as_mut().map(f)
}

pub fn set_hotkeys(h: &Hotkeys) {
    with_tracker(|t| t.set_hotkeys(h));
}

pub fn set_swallow_space(on: bool) {
    with_tracker(|t| t.set_swallow_space(on));
}

pub fn set_swallow_esc(on: bool) {
    with_tracker(|t| t.set_swallow_esc(on));
}

/// Turn full key tracing on/off (see [`TRACE`]).
pub fn set_trace(on: bool) {
    TRACE.store(on, Ordering::Relaxed);
}

pub fn user_key_counter() -> u64 {
    USER_KEYS.load(Ordering::Relaxed)
}

/// Re-send a swallowed chord to the foreground app (tap pass-through).
pub fn replay(kind: ChordKind) {
    if let Some(seq) = with_tracker(|t| t.replay_sequence(kind)) {
        send_keys(&seq);
    }
}

/// Stuck-key watchdog. Returns chord-up events for keys the OS says are released.
pub fn resync() -> Vec<HookEvent> {
    with_tracker(|t| t.resync(Instant::now(), os_key_down)).unwrap_or_default()
}

/// Modifier mask the user physically holds right now.
pub fn physical_mods() -> u8 {
    mods_from_flags(hid_flags())
}

fn source() -> Option<CFRetained<CGEventSource>> {
    CGEventSource::new(CGEventSourceStateID::Private)
}

/// Post one key event with explicit flags. Apps read modifiers from the event's
/// flags, so whatever the user still physically holds does not leak into it.
fn post(src: &CGEventSource, keycode: u16, down: bool, modifier: bool, flags: u64) -> bool {
    let Some(e) = CGEvent::new_keyboard_event(Some(src), keycode, down) else { return false };
    if modifier {
        CGEvent::set_type(Some(&e), CGEventType::FlagsChanged);
    }
    CGEvent::set_flags(Some(&e), CGEventFlags(flags));
    CGEvent::set_integer_value_field(Some(&e), CGEventField::EventSourceUserData, INJECT_MARKER as i64);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
    true
}

/// Inject key events, tagged so our own tap ignores them. Modifier events in the
/// sequence update the flags carried by the events after them.
pub fn send_keys(seq: &[KeyEvent]) -> bool {
    if seq.is_empty() {
        return true;
    }
    let Some(src) = source() else { return false };
    let mut flags = hid_flags() & 0x00FF_0000 & !CGEventFlags::MaskAlphaShift.0;
    let mut ok = true;
    for k in seq {
        let Some(keycode) = from_vk(k.vk) else {
            ok = false;
            continue;
        };
        let modifier = is_modifier(k.vk);
        if modifier {
            let bits = flags_for_mods(modifier_bit(k.vk)) | side_mask(k.vk);
            if k.down {
                flags |= bits;
            } else {
                flags &= !bits;
            }
        }
        ok &= post(&src, keycode, k.down, modifier, flags);
    }
    ok
}

/// Press a chord with exactly `mods_mask` held, whatever the user still holds.
/// `mods::CTRL` here means Control; callers that mean "the paste shortcut" pass `mods::WIN` (Command).
pub fn send_chord(mods_mask: u8, key: u16) -> bool {
    let Some(src) = source() else { return false };
    let Some(keycode) = from_vk(key) else { return false };
    let flags = flags_for_mods(mods_mask);
    post(&src, keycode, true, false, flags) && post(&src, keycode, false, false, flags)
}

/// Type text as Unicode key events (for terminals and apps that refuse paste).
pub fn send_unicode(text: &str) -> bool {
    let Some(src) = source() else { return false };
    let return_key = mac_keys::from_vk(vk::RETURN).unwrap_or(0x24);
    let mut ok = true;
    let units: Vec<u16> = text.encode_utf16().filter(|&u| u != '\r' as u16).collect();
    // CGEventKeyboardSetUnicodeString handles at most 20 UTF-16 units per event.
    for line in units.split(|&u| u == '\n' as u16).enumerate() {
        if line.0 > 0 {
            ok &= post(&src, return_key, true, false, 0) && post(&src, return_key, false, false, 0);
        }
        for chunk in line.1.chunks(20) {
            for down in [true, false] {
                let Some(e) = CGEvent::new_keyboard_event(Some(&src), 0, down) else {
                    ok = false;
                    continue;
                };
                unsafe { CGEvent::keyboard_set_unicode_string(Some(&e), chunk.len() as _, chunk.as_ptr()) };
                CGEvent::set_flags(Some(&e), CGEventFlags(0));
                CGEvent::set_integer_value_field(Some(&e), CGEventField::EventSourceUserData, INJECT_MARKER as i64);
                CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&e));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_decode_sides() {
        let lctrl = 0x0004_0000 | 0x1;
        assert_eq!(down_in_flags(lctrl, vk::LCONTROL), Some(true));
        assert_eq!(down_in_flags(lctrl, vk::RCONTROL), Some(false));
        assert_eq!(down_in_flags(lctrl, vk::CONTROL), Some(true));
        assert_eq!(down_in_flags(0x0080_0000, vk::FN), Some(true));
        assert_eq!(down_in_flags(0, vk::SPACE), None);
    }

    #[test]
    fn mask_covers_keys_and_flags() {
        assert_eq!(event_mask(), (1 << 10) | (1 << 11) | (1 << 12));
    }

    #[test]
    fn os_state_queries_do_not_panic() {
        let _ = physical_mods();
        let _ = os_key_down(vk::SPACE);
    }
}
