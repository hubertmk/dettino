//! Pure chord tracker run inside the low-level hook callback.
//!
//! It decides synchronously whether a key event is swallowed and which chord
//! event (if any) it produces. Keys that are not part of a configured chord are
//! passed through without being stored anywhere except the 256-entry "held"
//! table needed to compute modifier state.

use app_core::keys::{is_modifier, modifier_bit, mods, vk, ChordSpec, Hotkeys};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A swallowed key never reaches the OS key-state table, so the watchdog cannot ask
/// the OS about it. While physically held it keeps sending autorepeat downs to the
/// hook (Windows' slowest settings: 1 s delay, ~400 ms period), so silence for longer
/// than this means the key-up was lost.
pub const SWALLOWED_KEY_TIMEOUT: Duration = Duration::from_millis(1500);

/// A modifier the OS reports as released is dropped from the table only when it has
/// not produced a key-down (first or autorepeat) for this long. A modifier pressed
/// moments ago is part of the chord being typed; if the OS disagrees, the OS reading
/// (UIPI, another hook, a key-state race) is the suspect, not the hook.
pub const STALE_MODIFIER: Duration = Duration::from_millis(1500);

const MODIFIER_CODES: [u16; 11] = [
    vk::CONTROL, vk::LCONTROL, vk::RCONTROL, vk::SHIFT, vk::LSHIFT, vk::RSHIFT, vk::MENU, vk::LMENU, vk::RMENU,
    vk::LWIN, vk::RWIN,
];
/// Side-specific codes, the only ones a low-level hook ever delivers; generic
/// `VK_CONTROL`/`VK_SHIFT`/`VK_MENU` are never adopted from the OS because their
/// release would not be seen as a matching key-up.
const SIDE_CODES: [u16; 8] = [vk::LCONTROL, vk::RCONTROL, vk::LSHIFT, vk::RSHIFT, vk::LMENU, vk::RMENU, vk::LWIN, vk::RWIN];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordKind {
    PushToTalk,
    HandsFree,
    Command,
    UndoAi,
    PasteLast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u16,
    pub down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    ChordDown(ChordKind),
    ChordUp(ChordKind),
    /// A modifier-only chord turned out to be part of another shortcut.
    ChordAborted(ChordKind),
    BareSpace,
    Esc,
    /// Any configured chord modifier went down (opens the microphone early).
    ModifierDown,
    /// A chord modifier turned out not to start a chord: another key followed, or it
    /// was released alone.
    ModifierCancel,
    /// Swallow cap hit: hands-free stopped swallowing Space.
    SpaceSwallowCapHit,
    /// The first key event reached the hook (sent once per hook thread).
    HookAlive,
    /// What the hook saw for a key-down of a bound key (diagnostics; logged by the controller).
    KeyDiag(KeyDiag),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagOutcome {
    Matched(ChordKind),
    /// Modifiers were held but no chord has that combination.
    NoMatch,
    /// Swallowed without a chord event (autorepeat, bare Space in hands-free, Esc).
    Swallowed,
    /// Plain key, passed to the app.
    Passed,
}

/// One bound key-down as the hook saw it: the modifier table before the OS was
/// consulted, what the OS reported, and what the tracker decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyDiag {
    pub vk: u16,
    /// `KBDLLHOOKSTRUCT.flags` (injected, extended, alt-down, up).
    pub flags: u32,
    pub tracked_mods: u8,
    pub os_mods: u8,
    pub outcome: DiagOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Decision {
    pub swallow: bool,
    pub event: Option<HookEvent>,
}

impl Decision {
    fn pass() -> Self {
        Self::default()
    }
    fn swallow(event: Option<HookEvent>) -> Self {
        Self { swallow: true, event }
    }
    fn event(event: HookEvent) -> Self {
        Self { swallow: false, event: Some(event) }
    }
}

pub struct ChordTracker {
    bindings: Vec<(ChordKind, ChordSpec)>,
    held: [bool; 256],
    /// Keys whose key-down we swallowed: their key-up is swallowed too.
    swallowed_down: [bool; 256],
    /// Last down (first or autorepeat) seen for each key.
    last_seen: [Option<Instant>; 256],
    /// Terminal-key chord currently held (key was swallowed).
    active: Option<(ChordKind, u16)>,
    /// Modifier-only chord currently held.
    modonly: Option<(ChordKind, u16)>,
    swallow_space: bool,
    swallow_esc: bool,
    recent_spaces: VecDeque<Instant>,
    chord_modifiers: u8,
}

impl ChordTracker {
    pub fn new(hotkeys: &Hotkeys) -> Self {
        let mut t = Self {
            bindings: Vec::new(),
            held: [false; 256],
            swallowed_down: [false; 256],
            last_seen: [None; 256],
            active: None,
            modonly: None,
            swallow_space: false,
            swallow_esc: false,
            recent_spaces: VecDeque::new(),
            chord_modifiers: 0,
        };
        t.set_hotkeys(hotkeys);
        t
    }

    pub fn set_hotkeys(&mut self, h: &Hotkeys) {
        self.bindings = vec![
            (ChordKind::PushToTalk, h.push_to_talk),
            (ChordKind::HandsFree, h.hands_free),
            (ChordKind::Command, h.command),
            (ChordKind::UndoAi, h.undo_ai),
            (ChordKind::PasteLast, h.paste_last),
        ];
        self.chord_modifiers = self.bindings.iter().fold(0, |acc, (_, c)| acc | c.mods);
    }

    pub fn set_swallow_space(&mut self, on: bool) {
        self.swallow_space = on;
        self.recent_spaces.clear();
    }

    pub fn set_swallow_esc(&mut self, on: bool) {
        self.swallow_esc = on;
    }

    /// Current modifier mask from held keys.
    pub fn mods(&self) -> u8 {
        let mut m = 0;
        for code in [
            vk::CONTROL, vk::LCONTROL, vk::RCONTROL, vk::SHIFT, vk::LSHIFT, vk::RSHIFT, vk::MENU,
            vk::LMENU, vk::RMENU, vk::LWIN, vk::RWIN,
        ] {
            if self.held[code as usize] {
                m |= modifier_bit(code);
            }
        }
        m
    }

    /// Compare the modifier table with the OS before matching a key-down of a bound
    /// key. Returns (mask from the table before reconciling, mask the OS reports).
    ///
    /// The hook is first in the chain, so a key-down it saw is real and the table is
    /// authoritative. The OS state is used only to (a) adopt a modifier pressed while
    /// the hook was blind (elevated window, lock screen) and (b) release one whose
    /// key-up another hook ate — and (b) only once it has been silent for
    /// [`STALE_MODIFIER`], so a chord typed just now is never torn apart by a wrong
    /// OS reading. `except` is the key being processed, whose OS state is not updated yet.
    pub fn reconcile_modifiers(&mut self, except: u16, now: Instant, is_down: impl Fn(u16) -> bool) -> (u8, u8) {
        let tracked = self.mods();
        let mut os = 0u8;
        for code in MODIFIER_CODES {
            if code == except {
                continue;
            }
            let down = is_down(code);
            if down {
                os |= modifier_bit(code);
            }
            let i = code as usize;
            if down && !self.held[i] {
                if SIDE_CODES.contains(&code) {
                    self.held[i] = true;
                    self.last_seen[i] = Some(now);
                }
            } else if !down && self.held[i] && !self.seen_within(i, now, STALE_MODIFIER) {
                self.held[i] = false;
            }
        }
        (tracked, os)
    }

    fn seen_within(&self, i: usize, now: Instant, window: Duration) -> bool {
        self.last_seen[i].is_some_and(|t| now.saturating_duration_since(t) < window)
    }

    /// Is this key the terminal key of a configured chord?
    pub fn is_bound_key(&self, code: u16) -> bool {
        code != 0 && self.bindings.iter().any(|(_, s)| s.key == code)
    }

    pub fn is_held(&self, code: u16) -> bool {
        self.held[(code & 0xFF) as usize]
    }

    fn any_non_modifier_held(&self, except: u16) -> bool {
        (0..256u16).any(|k| k != except && self.held[k as usize] && !is_modifier(k))
    }

    pub fn on_key(&mut self, ev: KeyEvent, now: Instant) -> Decision {
        let code = (ev.vk & 0xFF) as usize;
        if ev.down {
            let was_held = self.held[code];
            self.held[code] = true;
            self.last_seen[code] = Some(now);
            self.on_down(ev.vk, was_held, now)
        } else {
            self.held[code] = false;
            self.on_up(ev.vk)
        }
    }

    fn on_down(&mut self, key: u16, autorepeat: bool, now: Instant) -> Decision {
        let code = (key & 0xFF) as usize;

        // Autorepeat of a key whose first down we swallowed.
        if autorepeat && self.swallowed_down[code] {
            return Decision::swallow(None);
        }

        // Esc cancels a session whatever chord started it, including a modifier-only one.
        if key == vk::ESCAPE && self.swallow_esc && !autorepeat {
            self.swallowed_down[code] = true;
            return Decision::swallow(Some(HookEvent::Esc));
        }

        // A modifier-only chord is being held and another key arrives: it was a shortcut.
        if let Some((kind, mkey)) = self.modonly {
            if key != mkey && !autorepeat {
                self.modonly = None;
                return Decision::event(HookEvent::ChordAborted(kind));
            }
        }

        let current = self.mods();

        // A key that was already down when swallowing started keeps autorepeating
        // unswallowed; those repeats are not presses.
        if key == vk::SPACE && self.swallow_space && current == 0 && self.active.is_none() && !autorepeat {
            // Safety cap: never swallow Space unboundedly (avoids a stuck-key failure mode).
            while let Some(&t0) = self.recent_spaces.front() {
                if now.saturating_duration_since(t0) > Duration::from_secs(2) {
                    self.recent_spaces.pop_front();
                } else {
                    break;
                }
            }
            if self.recent_spaces.len() >= 3 {
                self.swallow_space = false;
                self.recent_spaces.clear();
                return Decision::event(HookEvent::SpaceSwallowCapHit);
            }
            self.recent_spaces.push_back(now);
            self.swallowed_down[code] = true;
            return Decision::swallow(Some(HookEvent::BareSpace));
        }

        if is_modifier(key) {
            // Modifier-only chords (e.g. Right Ctrl alone).
            if !autorepeat {
                for &(kind, spec) in &self.bindings {
                    if spec.is_modifier_only() && spec.key == key {
                        let others = current & !modifier_bit(key);
                        if others == 0 && !self.any_non_modifier_held(key) {
                            self.modonly = Some((kind, key));
                            return Decision::event(HookEvent::ChordDown(kind));
                        }
                    }
                }
                if modifier_bit(key) & self.chord_modifiers != 0 {
                    return Decision::event(HookEvent::ModifierDown);
                }
            }
            return Decision::pass();
        }

        // One chord at a time: while a swallowed chord key is held, another bound key
        // is an ordinary key. Replacing `active` would lose the first chord's key-up.
        if autorepeat || self.active.is_some() {
            return Decision::pass();
        }

        // Caps Lock as a modifier-only chord: swallow both edges so the lock never toggles.
        if key == vk::CAPITAL {
            for &(kind, spec) in &self.bindings {
                if spec.is_modifier_only() && spec.key == vk::CAPITAL && current == 0 {
                    if autorepeat {
                        return Decision::swallow(None);
                    }
                    self.swallowed_down[code] = true;
                    self.active = Some((kind, key));
                    return Decision::swallow(Some(HookEvent::ChordDown(kind)));
                }
            }
        }

        // Terminal-key chords: exact modifier match.
        for &(kind, spec) in &self.bindings {
            if !spec.is_modifier_only() && spec.key == key && spec.mods == current {
                self.swallowed_down[code] = true;
                self.active = Some((kind, key));
                return Decision::swallow(Some(HookEvent::ChordDown(kind)));
            }
        }
        if current & self.chord_modifiers != 0 {
            return Decision::event(HookEvent::ModifierCancel);
        }
        Decision::pass()
    }

    fn on_up(&mut self, key: u16) -> Decision {
        let code = (key & 0xFF) as usize;
        if let Some((kind, mkey)) = self.modonly {
            if key == mkey {
                self.modonly = None;
                return Decision::event(HookEvent::ChordUp(kind));
            }
        }
        if self.swallowed_down[code] {
            self.swallowed_down[code] = false;
            if let Some((kind, akey)) = self.active {
                if akey == key {
                    self.active = None;
                    return Decision::swallow(Some(HookEvent::ChordUp(kind)));
                }
            }
            return Decision::swallow(None);
        }
        if is_modifier(key)
            && modifier_bit(key) & self.chord_modifiers != 0
            && self.mods() & self.chord_modifiers == 0
            && self.active.is_none()
        {
            return Decision::event(HookEvent::ModifierCancel);
        }
        Decision::pass()
    }

    /// Keys to inject to replay a swallowed chord as the user typed it.
    /// Modifiers the user no longer physically holds are bracketed around the key.
    pub fn replay_sequence(&self, kind: ChordKind) -> Vec<KeyEvent> {
        let Some(&(_, spec)) = self.bindings.iter().find(|(k, _)| *k == kind) else {
            return Vec::new();
        };
        if spec.is_modifier_only() && spec.key != vk::CAPITAL {
            return Vec::new(); // nothing was swallowed
        }
        let held = self.mods();
        let mut pre = Vec::new();
        for (bit, code) in [
            (mods::CTRL, vk::LCONTROL),
            (mods::SHIFT, vk::LSHIFT),
            (mods::ALT, vk::LMENU),
            (mods::WIN, vk::LWIN),
        ] {
            if spec.mods & bit != 0 && held & bit == 0 {
                pre.push(code);
            }
        }
        let mut seq: Vec<KeyEvent> = pre.iter().map(|&c| KeyEvent { vk: c, down: true }).collect();
        seq.push(KeyEvent { vk: spec.key, down: true });
        seq.push(KeyEvent { vk: spec.key, down: false });
        seq.extend(pre.iter().rev().map(|&c| KeyEvent { vk: c, down: false }));
        seq
    }

    /// Stuck-key watchdog: compare our table with the OS key state.
    /// Returns events for chords that were released without us seeing the key-up.
    /// Swallowed keys are invisible to the OS, so they count as released only after
    /// [`SWALLOWED_KEY_TIMEOUT`] without autorepeat. A modifier the OS reports
    /// released is kept while it is fresh (see [`STALE_MODIFIER`]).
    pub fn resync(&mut self, now: Instant, is_down: impl Fn(u16) -> bool) -> Vec<HookEvent> {
        let mut out = Vec::new();
        // Autorepeat moves to the last key pressed, so a swallowed chord key stops
        // repeating as soon as the user taps anything else. While one of the chord's own
        // modifiers (which the OS does see) is still down, the chord is still held.
        let active_mods_down = self.active.and_then(|(kind, _)| self.bindings.iter().find(|(k, _)| *k == kind)).is_some_and(|(_, spec)| {
            [(mods::CTRL, vk::CONTROL), (mods::SHIFT, vk::SHIFT), (mods::ALT, vk::MENU)]
                .iter()
                .any(|(bit, generic)| spec.mods & bit != 0 && is_down(*generic))
                || (spec.mods & mods::WIN != 0 && (is_down(vk::LWIN) || is_down(vk::RWIN)))
        });
        for code in 0..256u16 {
            let i = code as usize;
            let is_active_key = self.active.is_some_and(|(_, k)| k == code);
            let still_down = if self.swallowed_down[i] {
                self.seen_within(i, now, SWALLOWED_KEY_TIMEOUT) || (is_active_key && active_mods_down)
            } else if is_modifier(code) {
                is_down(code) || self.seen_within(i, now, STALE_MODIFIER)
            } else {
                is_down(code)
            };
            if self.held[i] && !still_down {
                self.held[code as usize] = false;
                self.swallowed_down[code as usize] = false;
                if let Some((kind, k)) = self.active {
                    if k == code {
                        self.active = None;
                        out.push(HookEvent::ChordUp(kind));
                    }
                }
                if let Some((kind, k)) = self.modonly {
                    if k == code {
                        self.modonly = None;
                        out.push(HookEvent::ChordUp(kind));
                    }
                }
            }
        }
        out
    }

    /// Forget all key state (the hook was reinstalled). Returns the key-ups the
    /// caller must deliver for chords that were held, so a session never stays open.
    pub fn reset(&mut self) -> Vec<HookEvent> {
        let mut out = Vec::new();
        if let Some((kind, _)) = self.active.take() {
            out.push(HookEvent::ChordUp(kind));
        }
        if let Some((kind, _)) = self.modonly.take() {
            out.push(HookEvent::ChordUp(kind));
        }
        self.held = [false; 256];
        self.swallowed_down = [false; 256];
        self.last_seen = [None; 256];
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr() -> ChordTracker {
        ChordTracker::new(&Hotkeys::default())
    }
    fn down(t: &mut ChordTracker, k: u16) -> Decision {
        t.on_key(KeyEvent { vk: k, down: true }, Instant::now())
    }
    fn up(t: &mut ChordTracker, k: u16) -> Decision {
        t.on_key(KeyEvent { vk: k, down: false }, Instant::now())
    }

    #[test]
    fn ctrl_space_is_swallowed_and_reported() {
        let mut t = tr();
        let d = down(&mut t, vk::LCONTROL);
        assert!(!d.swallow);
        assert_eq!(d.event, Some(HookEvent::ModifierDown));
        let d = down(&mut t, vk::SPACE);
        assert!(d.swallow);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
        // autorepeat swallowed silently
        let d = down(&mut t, vk::SPACE);
        assert_eq!(d, Decision { swallow: true, event: None });
        let d = up(&mut t, vk::SPACE);
        assert_eq!(d, Decision { swallow: true, event: Some(HookEvent::ChordUp(ChordKind::PushToTalk)) });
        assert!(!up(&mut t, vk::LCONTROL).swallow);
    }

    #[test]
    fn exact_modifier_match_distinguishes_chords() {
        let mut t = tr();
        down(&mut t, vk::RCONTROL);
        down(&mut t, vk::LSHIFT);
        let d = down(&mut t, vk::SPACE);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::HandsFree)));
        up(&mut t, vk::SPACE);
        up(&mut t, vk::LSHIFT);
        down(&mut t, vk::LMENU);
        let d = down(&mut t, vk::SPACE);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::Command)));
    }

    #[test]
    fn plain_space_and_other_keys_pass() {
        let mut t = tr();
        assert_eq!(down(&mut t, vk::SPACE), Decision::pass());
        assert_eq!(up(&mut t, vk::SPACE), Decision::pass());
        down(&mut t, vk::LCONTROL);
        assert_eq!(down(&mut t, vk::KEY_C), Decision::event(HookEvent::ModifierCancel));
        assert_eq!(up(&mut t, vk::KEY_C), Decision::pass());
    }

    #[test]
    fn modifier_released_alone_cancels() {
        let mut t = tr();
        assert_eq!(down(&mut t, vk::LCONTROL).event, Some(HookEvent::ModifierDown));
        assert_eq!(up(&mut t, vk::LCONTROL), Decision::event(HookEvent::ModifierCancel));
        // unrelated modifiers never cancel
        let ctrl_space = ChordSpec::new(mods::CTRL, vk::SPACE);
        let none = ChordSpec::new(0, 0);
        let mut t = ChordTracker::new(&Hotkeys { push_to_talk: ctrl_space, hands_free: none, command: none, undo_ai: none, paste_last: none });
        down(&mut t, vk::LSHIFT);
        assert_eq!(up(&mut t, vk::LSHIFT), Decision::pass());
    }

    #[test]
    fn no_cancel_while_a_chord_is_held() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::SPACE);
        assert_eq!(up(&mut t, vk::LCONTROL), Decision::pass());
        // Ctrl+Shift+Space: the second modifier is not a cancel either
        let mut t = tr();
        down(&mut t, vk::RCONTROL);
        assert_eq!(down(&mut t, vk::LSHIFT).event, Some(HookEvent::ModifierDown));
        assert_eq!(down(&mut t, vk::SPACE).event, Some(HookEvent::ChordDown(ChordKind::HandsFree)));
    }

    #[test]
    fn ctrl_released_before_space_still_ends_chord() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::SPACE);
        up(&mut t, vk::LCONTROL);
        let d = up(&mut t, vk::SPACE);
        assert_eq!(d.event, Some(HookEvent::ChordUp(ChordKind::PushToTalk)));
        assert!(d.swallow);
    }

    #[test]
    fn replay_brackets_released_modifier() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::SPACE);
        // ctrl still held → replay only Space
        let seq = t.replay_sequence(ChordKind::PushToTalk);
        assert_eq!(seq, vec![KeyEvent { vk: vk::SPACE, down: true }, KeyEvent { vk: vk::SPACE, down: false }]);
        up(&mut t, vk::LCONTROL);
        let seq = t.replay_sequence(ChordKind::PushToTalk);
        assert_eq!(seq.first(), Some(&KeyEvent { vk: vk::LCONTROL, down: true }));
        assert_eq!(seq.last(), Some(&KeyEvent { vk: vk::LCONTROL, down: false }));
        assert_eq!(seq.len(), 4);
    }

    #[test]
    fn bare_space_only_when_enabled_and_capped() {
        let mut t = tr();
        t.set_swallow_space(true);
        let now = Instant::now();
        for i in 0..3 {
            let d = t.on_key(KeyEvent { vk: vk::SPACE, down: true }, now + Duration::from_millis(i * 100));
            assert_eq!(d.event, Some(HookEvent::BareSpace));
            assert!(t.on_key(KeyEvent { vk: vk::SPACE, down: false }, now).swallow);
        }
        let d = t.on_key(KeyEvent { vk: vk::SPACE, down: true }, now + Duration::from_millis(400));
        assert_eq!(d.event, Some(HookEvent::SpaceSwallowCapHit));
        assert!(!d.swallow);
    }

    #[test]
    fn shift_space_is_not_bare_space() {
        let mut t = tr();
        t.set_swallow_space(true);
        down(&mut t, vk::LSHIFT);
        let d = down(&mut t, vk::SPACE);
        assert!(!d.swallow);
        assert_ne!(d.event, Some(HookEvent::BareSpace));
    }

    #[test]
    fn esc_only_swallowed_during_session() {
        let mut t = tr();
        assert_eq!(down(&mut t, vk::ESCAPE), Decision::pass());
        up(&mut t, vk::ESCAPE);
        t.set_swallow_esc(true);
        assert_eq!(down(&mut t, vk::ESCAPE).event, Some(HookEvent::Esc));
        assert!(up(&mut t, vk::ESCAPE).swallow);
    }

    #[test]
    fn modifier_only_chord_and_abort() {
        let h = Hotkeys { push_to_talk: ChordSpec::new(0, vk::RCONTROL), ..Hotkeys::default() };
        let mut t = ChordTracker::new(&h);
        let d = down(&mut t, vk::RCONTROL);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
        assert!(!d.swallow);
        let d = down(&mut t, vk::KEY_C);
        assert_eq!(d.event, Some(HookEvent::ChordAborted(ChordKind::PushToTalk)));
        assert!(!d.swallow);
        up(&mut t, vk::KEY_C);
        assert!(!up(&mut t, vk::RCONTROL).swallow);
    }

    #[test]
    fn caps_lock_chord_swallows_both_edges() {
        let h = Hotkeys { push_to_talk: ChordSpec::new(0, vk::CAPITAL), ..Hotkeys::default() };
        let mut t = ChordTracker::new(&h);
        let d = down(&mut t, vk::CAPITAL);
        assert_eq!(d, Decision { swallow: true, event: Some(HookEvent::ChordDown(ChordKind::PushToTalk)) });
        let d = up(&mut t, vk::CAPITAL);
        assert_eq!(d, Decision { swallow: true, event: Some(HookEvent::ChordUp(ChordKind::PushToTalk)) });
    }

    #[test]
    fn resync_releases_stuck_chord() {
        let mut t = tr();
        let t0 = Instant::now();
        t.on_key(KeyEvent { vk: vk::LCONTROL, down: true }, t0);
        t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t0);
        let ev = t.resync(t0 + SWALLOWED_KEY_TIMEOUT, |_| false);
        assert_eq!(ev, vec![HookEvent::ChordUp(ChordKind::PushToTalk)]);
        assert_eq!(t.mods(), 0);
        // the eventual real key-up is not swallowed anymore
        assert_eq!(up(&mut t, vk::SPACE), Decision::pass());
    }

    #[test]
    fn stale_modifier_is_released_by_os_reconcile() {
        let mut t = tr();
        let t0 = Instant::now();
        // Shift key-up was eaten by another hook two seconds ago
        t.on_key(KeyEvent { vk: vk::LSHIFT, down: true }, t0);
        let t1 = t0 + Duration::from_secs(2);
        t.on_key(KeyEvent { vk: vk::LCONTROL, down: true }, t1);
        let (tracked, os) = t.reconcile_modifiers(vk::SPACE, t1, |k| k == vk::LCONTROL || k == vk::CONTROL);
        assert_eq!(tracked, mods::CTRL | mods::SHIFT);
        assert_eq!(os, mods::CTRL);
        let d = t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t1);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
    }

    #[test]
    fn fresh_modifier_survives_a_wrong_os_reading() {
        let mut t = tr();
        let t0 = Instant::now();
        t.on_key(KeyEvent { vk: vk::LCONTROL, down: true }, t0);
        let t1 = t0 + Duration::from_millis(300);
        // the OS claims nothing is held (UIPI, a race): the chord still fires
        assert_eq!(t.reconcile_modifiers(vk::SPACE, t1, |_| false), (mods::CTRL, 0));
        let d = t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t1);
        assert_eq!(d.event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
        assert!(d.swallow);
    }

    #[test]
    fn modifier_pressed_while_blind_is_adopted() {
        let mut t = tr();
        let now = Instant::now();
        let os = |k: u16| k == vk::RCONTROL || k == vk::CONTROL;
        assert_eq!(t.reconcile_modifiers(vk::SPACE, now, os), (0, mods::CTRL));
        assert_eq!(t.mods(), mods::CTRL);
        assert_eq!(down(&mut t, vk::SPACE).event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
        up(&mut t, vk::SPACE);
        // the eventual real key-up releases it; the generic VK_CONTROL was never adopted
        up(&mut t, vk::RCONTROL);
        assert_eq!(t.mods(), 0);
    }

    #[test]
    fn resync_keeps_a_fresh_modifier_and_drops_a_stale_one() {
        let mut t = tr();
        let t0 = Instant::now();
        t.on_key(KeyEvent { vk: vk::LSHIFT, down: true }, t0);
        assert!(t.resync(t0 + Duration::from_millis(500), |_| false).is_empty());
        assert_eq!(t.mods(), mods::SHIFT);
        t.resync(t0 + STALE_MODIFIER, |_| false);
        assert_eq!(t.mods(), 0);
    }

    #[test]
    fn bound_keys_follow_the_bindings() {
        let t = tr();
        assert!(t.is_bound_key(vk::SPACE));
        assert!(t.is_bound_key(vk::KEY_Z));
        assert!(!t.is_bound_key(vk::KEY_C));
        let none = ChordSpec::new(0, 0);
        let t = ChordTracker::new(&Hotkeys { push_to_talk: none, hands_free: none, command: none, undo_ai: none, paste_last: none });
        assert!(!t.is_bound_key(0));
    }

    #[test]
    fn resync_keeps_held_swallowed_key() {
        // The OS never sees a swallowed Space, so it always reports it as up.
        let mut t = tr();
        let t0 = Instant::now();
        let os = |k: u16| k == vk::LCONTROL;
        t.on_key(KeyEvent { vk: vk::LCONTROL, down: true }, t0);
        t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t0);
        assert!(t.resync(t0 + Duration::from_millis(500), os).is_empty());
        // autorepeat keeps it alive well past the timeout
        let t1 = t0 + Duration::from_millis(1200);
        assert_eq!(t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t1), Decision::swallow(None));
        assert!(t.resync(t0 + Duration::from_millis(2000), os).is_empty());
        let d = t.on_key(KeyEvent { vk: vk::SPACE, down: false }, t0 + Duration::from_millis(2100));
        assert_eq!(d.event, Some(HookEvent::ChordUp(ChordKind::PushToTalk)));
    }

    #[test]
    fn a_second_bound_key_during_a_chord_is_an_ordinary_key() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::LMENU);
        assert_eq!(down(&mut t, vk::SPACE).event, Some(HookEvent::ChordDown(ChordKind::Command)));
        // V while Ctrl+Alt+Space is held: passed through, the command chord stays active
        assert_eq!(down(&mut t, vk::KEY_V), Decision::pass());
        assert_eq!(up(&mut t, vk::KEY_V), Decision::pass());
        let d = up(&mut t, vk::SPACE);
        assert_eq!(d, Decision { swallow: true, event: Some(HookEvent::ChordUp(ChordKind::Command)) });
    }

    #[test]
    fn esc_cancels_a_modifier_only_chord() {
        let h = Hotkeys { push_to_talk: ChordSpec::new(0, vk::RCONTROL), ..Hotkeys::default() };
        let mut t = ChordTracker::new(&h);
        assert_eq!(down(&mut t, vk::RCONTROL).event, Some(HookEvent::ChordDown(ChordKind::PushToTalk)));
        t.set_swallow_esc(true);
        let d = down(&mut t, vk::ESCAPE);
        assert_eq!(d, Decision { swallow: true, event: Some(HookEvent::Esc) });
    }

    #[test]
    fn a_key_held_before_swallowing_started_is_not_a_press() {
        let mut t = tr();
        // Space is physically down (unswallowed) when hands-free turns swallowing on
        down(&mut t, vk::SPACE);
        t.set_swallow_space(true);
        // its autorepeat must not count as "stop" presses
        assert_eq!(down(&mut t, vk::SPACE), Decision::pass());
        assert_eq!(down(&mut t, vk::SPACE), Decision::pass());
        up(&mut t, vk::SPACE);
        assert_eq!(down(&mut t, vk::SPACE).event, Some(HookEvent::BareSpace));
    }

    #[test]
    fn resync_keeps_a_chord_while_its_modifier_is_still_down() {
        let mut t = tr();
        let t0 = Instant::now();
        t.on_key(KeyEvent { vk: vk::LCONTROL, down: true }, t0);
        t.on_key(KeyEvent { vk: vk::SPACE, down: true }, t0);
        // The user tapped Shift: Space stops autorepeating, but Ctrl is still held.
        let ctrl_held = |k: u16| k == vk::CONTROL || k == vk::LCONTROL;
        assert!(t.resync(t0 + Duration::from_secs(5), ctrl_held).is_empty());
        // Ctrl released too: now the lost key-up is assumed.
        let ev = t.resync(t0 + Duration::from_secs(5), |_| false);
        assert_eq!(ev, vec![HookEvent::ChordUp(ChordKind::PushToTalk)]);
    }

    #[test]
    fn reset_reports_the_chord_it_drops() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::SPACE);
        assert_eq!(t.reset(), vec![HookEvent::ChordUp(ChordKind::PushToTalk)]);
        assert!(t.reset().is_empty());
        assert_eq!(t.mods(), 0);
    }

    #[test]
    fn undo_and_paste_last_chords() {
        let mut t = tr();
        down(&mut t, vk::LCONTROL);
        down(&mut t, vk::LMENU);
        assert_eq!(down(&mut t, vk::KEY_Z).event, Some(HookEvent::ChordDown(ChordKind::UndoAi)));
        up(&mut t, vk::KEY_Z);
        assert_eq!(down(&mut t, vk::KEY_V).event, Some(HookEvent::ChordDown(ChordKind::PasteLast)));
    }
}
