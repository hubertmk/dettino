//! Windows backend: `WH_KEYBOARD_LL` on a dedicated thread with a `GetMessage`
//! pump, plus `SendInput`-based injection tagged with [`INJECT_MARKER`].
//!
//! The hook callback keeps its work small: it consults the tracker, counts keys,
//! and forwards events (including a one-time `HookAlive` and per-key `KeyDiag`
//! diagnostics) over a channel the controller logs. Logging and the foreground
//! lookup happen on the controller thread, never in the callback.

use crate::tracker::{ChordKind, ChordTracker, DiagOutcome, HookEvent, KeyDiag, KeyEvent};
use crate::INJECT_MARKER;
use app_core::keys::{vk, Hotkeys};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static TRACKER: Mutex<Option<ChordTracker>> = Mutex::new(None);
static SENDER: OnceLock<Sender<(HookEvent, Instant)>> = OnceLock::new();
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// Counts real (non-injected, non-modifier, not swallowed) key-downs: did the user type since a paste?
static USER_KEYS: AtomicU64 = AtomicU64::new(0);
/// First event seen (reported once as `HookEvent::HookAlive`).
static HOOK_ALIVE: AtomicBool = AtomicBool::new(false);
/// When set, every key-down produces a `KeyDiag`, not only bound keys. Used by the
/// "Test your shortcut" panel to show the full key sequence, including a modifier
/// the user presses that the hook may never receive.
static TRACE: AtomicBool = AtomicBool::new(false);

/// AltGr arrives as a fake Left Ctrl (scan code 0x21D) right before Right Alt. Fed to
/// the tracker it reads as Ctrl+Alt, so AltGr+V (`@` on some layouts) would match a
/// Ctrl+Alt+V chord and be swallowed. The fake Ctrl is ignored; Right Alt still counts.
fn is_altgr_marker(vk_code: u16, scan_code: u32) -> bool {
    vk_code == vk::LCONTROL && scan_code == 0x21D
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let ours = (kb.flags.0 & LLKHF_INJECTED.0) != 0 && kb.dwExtraInfo == INJECT_MARKER;
        if !ours && !is_altgr_marker(kb.vkCode as u16, kb.scanCode) {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            if down || up {
                let now = Instant::now();
                let vk_code = kb.vkCode as u16;
                let tx = SENDER.get();
                if !HOOK_ALIVE.swap(true, Ordering::Relaxed) {
                    if let Some(tx) = tx {
                        let _ = tx.send((HookEvent::HookAlive, now));
                    }
                }
                let trace = TRACE.load(Ordering::Relaxed);
                let mut diag = None;
                // A poisoned lock (a panic elsewhere while it was held) must not switch
                // every shortcut off for the rest of the session.
                let mut guard = TRACKER.lock().unwrap_or_else(|p| p.into_inner());
                let decision = {
                    guard.as_mut().map(|t| {
                        let diagworthy = down && (t.is_bound_key(vk_code) || trace);
                        let repeat = t.is_held(vk_code);
                        let masks = if diagworthy { Some(t.reconcile_modifiers(vk_code, now, os_key_down)) } else { None };
                        let d = t.on_key(KeyEvent { vk: vk_code, down }, now);
                        if let (Some((tracked, os)), false) = (masks, repeat) {
                            let outcome = match d.event {
                                Some(HookEvent::ChordDown(kind)) => DiagOutcome::Matched(kind),
                                _ if d.swallow => DiagOutcome::Swallowed,
                                _ if (tracked | os) != 0 => DiagOutcome::NoMatch,
                                _ => DiagOutcome::Passed,
                            };
                            diag = Some(KeyDiag { vk: vk_code, flags: kb.flags.0, tracked_mods: tracked, os_mods: os, outcome });
                        }
                        d
                    })
                };
                drop(guard);
                if down
                    && !decision.map(|d| d.swallow).unwrap_or(false)
                    && !app_core::keys::is_modifier(vk_code)
                {
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
                if decision.is_some_and(|d| d.swallow) {
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Start the hook thread. Events arrive on the returned receiver.
pub fn start(hotkeys: &Hotkeys) -> Result<Receiver<(HookEvent, Instant)>, String> {
    if SENDER.get().is_some() {
        return Err("hotkey hook already started".into());
    }
    let (tx, rx) = unbounded();
    *TRACKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(ChordTracker::new(hotkeys));

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("hotkey-hook".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(None).ok();
            let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod.map(|h| h.into()), 0) {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("SetWindowsHookExW failed: {e}")));
                    return;
                }
            };
            HOOK_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
            let _ = ready_tx.send(Ok(()));
            let mut msg = MSG::default();
            // GetMessageW returns -1 on error; only 0 (WM_QUIT) ends the pump.
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(hook);
            HOOK_THREAD.store(0, Ordering::SeqCst);
        })
        .map_err(|e| e.to_string())?;
    ready_rx.recv().map_err(|e| e.to_string())??;
    // Only a hook that is installed gets the sender: a failed install leaves the
    // slot free for the next attempt (the few events before this point are dropped).
    let _ = SENDER.set(tx);
    Ok(rx)
}

/// True once the hook is installed.
pub fn is_active() -> bool {
    HOOK_THREAD.load(Ordering::SeqCst) != 0
}

pub fn stop() {
    let tid = HOOK_THREAD.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

fn with_tracker<R>(f: impl FnOnce(&mut ChordTracker) -> R) -> Option<R> {
    TRACKER.lock().unwrap_or_else(|p| p.into_inner()).as_mut().map(f)
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

fn os_key_down(code: u16) -> bool {
    unsafe { (GetAsyncKeyState(code as i32) as u16 & 0x8000) != 0 }
}

/// Stuck-key watchdog. Returns chord-up events for keys the OS says are released.
pub fn resync() -> Vec<HookEvent> {
    with_tracker(|t| t.resync(Instant::now(), os_key_down)).unwrap_or_default()
}

/// Modifier mask the user physically holds right now.
pub fn physical_mods() -> u8 {
    use app_core::keys::mods;
    let mut m = 0;
    if os_key_down(vk::CONTROL) {
        m |= mods::CTRL;
    }
    if os_key_down(vk::SHIFT) {
        m |= mods::SHIFT;
    }
    if os_key_down(vk::MENU) {
        m |= mods::ALT;
    }
    if os_key_down(vk::LWIN) || os_key_down(vk::RWIN) {
        m |= mods::WIN;
    }
    m
}

fn is_extended(code: u16) -> bool {
    matches!(
        code,
        vk::RCONTROL | vk::RMENU | vk::INSERT | vk::LEFT | 0x21..=0x28 | 0x2E | vk::LWIN | vk::RWIN
    )
}

fn key_input(code: u16, down: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(code as u32, MAPVK_VK_TO_VSC) } as u16;
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(code) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(code),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECT_MARKER,
            },
        },
    }
}

/// Inject key events, tagged so our own hook ignores them.
pub fn send_keys(seq: &[KeyEvent]) -> bool {
    if seq.is_empty() {
        return true;
    }
    let inputs: Vec<INPUT> = seq.iter().map(|k| key_input(k.vk, k.down)).collect();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

/// Modifier bits with their left and right virtual keys.
const SIDES: [(u8, u16, u16); 4] = [
    (app_core::keys::mods::CTRL, vk::LCONTROL, vk::RCONTROL),
    (app_core::keys::mods::SHIFT, vk::LSHIFT, vk::RSHIFT),
    (app_core::keys::mods::ALT, vk::LMENU, vk::RMENU),
    (app_core::keys::mods::WIN, vk::LWIN, vk::RWIN),
];

/// Key-up events for every modifier side the user physically holds, except the
/// bits in `keep`. Returns the codes released so they can be pressed again after.
fn release_held_modifiers(keep: u8, seq: &mut Vec<KeyEvent>) -> Vec<u16> {
    let mut released = Vec::new();
    for (bit, l, r) in SIDES {
        if keep & bit != 0 {
            continue;
        }
        for c in [l, r] {
            if os_key_down(c) {
                seq.push(KeyEvent { vk: c, down: false });
                released.push(c);
            }
        }
    }
    released
}

/// Press a chord (e.g. Ctrl+V) after releasing any modifiers the user still holds,
/// then restore the user's physical modifier state.
///
/// Everything works per side: a user holding *Right* Ctrl through the paste must
/// not get an injected Left Ctrl that nothing ever releases (every Space would then
/// read as Ctrl+Space), and a held Right Shift must not reach the target as part of
/// the chord.
pub fn send_chord(mods_mask: u8, key: u16) -> bool {
    let mut seq = Vec::new();
    let released = release_held_modifiers(mods_mask, &mut seq);
    // Chord modifiers the user does not already hold on either side.
    let mut pressed = Vec::new();
    for (bit, l, r) in SIDES {
        if mods_mask & bit != 0 && !os_key_down(l) && !os_key_down(r) {
            seq.push(KeyEvent { vk: l, down: true });
            pressed.push(l);
        }
    }
    seq.push(KeyEvent { vk: key, down: true });
    seq.push(KeyEvent { vk: key, down: false });
    for c in pressed.iter().rev() {
        seq.push(KeyEvent { vk: *c, down: false });
    }
    // Put back what the user is still holding; their own key-up ends it later.
    for c in released {
        seq.push(KeyEvent { vk: c, down: true });
    }
    send_keys(&seq)
}

/// Type text as Unicode key events (for terminals, RDP and apps that refuse paste).
pub fn send_unicode(text: &str) -> bool {
    // Release held modifiers first so Ctrl+<char> shortcuts are not triggered, and
    // put them back afterwards.
    let mut pre = Vec::new();
    let released = release_held_modifiers(0, &mut pre);
    if !pre.is_empty() {
        send_keys(&pre);
    }
    let unit_input = |unit: u16, up: bool| {
        let mut flags = KEYEVENTF_UNICODE;
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: INJECT_MARKER },
            },
        }
    };
    let mut ok = true;
    let mut inputs: Vec<INPUT> = Vec::with_capacity(80);
    let flush = |inputs: &mut Vec<INPUT>, ok: &mut bool| {
        if inputs.is_empty() {
            return;
        }
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        *ok &= sent as usize == inputs.len();
        inputs.clear();
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    // Chunk on character boundaries: a surrogate pair split across two SendInput
    // calls arrives as two broken characters.
    for c in text.chars() {
        match c {
            '\r' => continue,
            '\n' => {
                inputs.push(key_input(vk::RETURN, true));
                inputs.push(key_input(vk::RETURN, false));
            }
            _ => {
                let mut units = [0u16; 2];
                for &unit in c.encode_utf16(&mut units).iter() {
                    inputs.push(unit_input(unit, false));
                    inputs.push(unit_input(unit, true));
                }
            }
        }
        if inputs.len() >= 64 {
            flush(&mut inputs, &mut ok);
        }
    }
    flush(&mut inputs, &mut ok);
    if !released.is_empty() {
        let back: Vec<KeyEvent> = released.into_iter().map(|c| KeyEvent { vk: c, down: true }).collect();
        send_keys(&back);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altgr_marker_is_the_fake_left_ctrl() {
        assert!(is_altgr_marker(vk::LCONTROL, 0x21D));
        assert!(!is_altgr_marker(vk::LCONTROL, 0x1D), "a real Left Ctrl has scan code 0x1D");
        assert!(!is_altgr_marker(vk::RMENU, 0x21D));
    }
}
