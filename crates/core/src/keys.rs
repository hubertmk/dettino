//! Chord specification shared by settings, the hook backends and the UI.
//!
//! Keys are identified by Windows virtual-key codes on every platform; the macOS
//! backend maps CGKeyCodes onto them. Matching never uses characters, so
//! non-QWERTY layouts work.

use serde::{Deserialize, Serialize};

pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12; // Alt
    pub const CAPITAL: u16 = 0x14; // Caps Lock
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const INSERT: u16 = 0x2D;
    pub const LEFT: u16 = 0x25;
    pub const KEY_C: u16 = 0x43;
    pub const KEY_V: u16 = 0x56;
    pub const KEY_Z: u16 = 0x5A;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const F13: u16 = 0x7C;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
    /// Synthetic code used for the macOS Fn key.
    pub const FN: u16 = 0xFF;
}

/// Modifier bitmask, side-independent.
pub mod mods {
    pub const CTRL: u8 = 1;
    pub const SHIFT: u8 = 2;
    pub const ALT: u8 = 4;
    pub const WIN: u8 = 8;
}

/// A chord: a set of held modifiers plus one terminal key.
///
/// When `key` is itself a modifier (e.g. Right Ctrl) or Caps Lock and `mods == 0`,
/// the chord is "modifier-only": it fires on that key alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChordSpec {
    pub mods: u8,
    pub key: u16,
}

impl ChordSpec {
    pub const fn new(mods: u8, key: u16) -> Self {
        Self { mods, key }
    }

    pub fn is_modifier_only(&self) -> bool {
        self.mods == 0 && (is_modifier(self.key) || self.key == vk::CAPITAL || self.key == vk::FN)
    }

    /// Human-readable label, e.g. "Ctrl + Shift + Space" ("Control + Shift + Space" on macOS).
    pub fn label(&self) -> String {
        self.label_for(MAC)
    }

    fn label_for(&self, mac: bool) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.mods & mods::CTRL != 0 {
            parts.push(if mac { "Control" } else { "Ctrl" });
        }
        if self.mods & mods::ALT != 0 {
            parts.push(if mac { "Option" } else { "Alt" });
        }
        if self.mods & mods::SHIFT != 0 {
            parts.push("Shift");
        }
        if self.mods & mods::WIN != 0 {
            parts.push(if mac { "Command" } else { "Win" });
        }
        let key = key_name_for(self.key, mac);
        let mut s = parts.join(" + ");
        if !s.is_empty() {
            s.push_str(" + ");
        }
        s.push_str(&key);
        s
    }
}

pub fn is_modifier(vk_code: u16) -> bool {
    matches!(
        vk_code,
        vk::SHIFT
            | vk::CONTROL
            | vk::MENU
            | vk::LSHIFT
            | vk::RSHIFT
            | vk::LCONTROL
            | vk::RCONTROL
            | vk::LMENU
            | vk::RMENU
            | vk::LWIN
            | vk::RWIN
            | vk::FN
    )
}

/// Modifier bit for a modifier VK, or 0.
pub fn modifier_bit(vk_code: u16) -> u8 {
    match vk_code {
        vk::CONTROL | vk::LCONTROL | vk::RCONTROL => mods::CTRL,
        vk::SHIFT | vk::LSHIFT | vk::RSHIFT => mods::SHIFT,
        vk::MENU | vk::LMENU | vk::RMENU => mods::ALT,
        vk::LWIN | vk::RWIN => mods::WIN,
        _ => 0,
    }
}

/// Labels use the macOS names (Control, Option, Command) on macOS.
const MAC: bool = cfg!(target_os = "macos");

pub fn key_name(vk_code: u16) -> String {
    key_name_for(vk_code, MAC)
}

fn key_name_for(vk_code: u16, mac: bool) -> String {
    if mac {
        let name = match vk_code {
            vk::LCONTROL => Some("Left Control"),
            vk::RCONTROL => Some("Right Control"),
            vk::LMENU => Some("Left Option"),
            vk::RMENU => Some("Right Option"),
            vk::LWIN => Some("Left Command"),
            vk::RWIN => Some("Right Command"),
            vk::RETURN => Some("Return"),
            vk::BACK => Some("Delete"),
            _ => None,
        };
        if let Some(n) = name {
            return n.into();
        }
    }
    match vk_code {
        vk::SPACE => "Space".into(),
        vk::RETURN => "Enter".into(),
        vk::TAB => "Tab".into(),
        vk::ESCAPE => "Esc".into(),
        vk::BACK => "Backspace".into(),
        vk::CAPITAL => "Caps Lock".into(),
        vk::LCONTROL => "Left Ctrl".into(),
        vk::RCONTROL => "Right Ctrl".into(),
        vk::LSHIFT => "Left Shift".into(),
        vk::RSHIFT => "Right Shift".into(),
        vk::LMENU => "Left Alt".into(),
        vk::RMENU => "Right Alt".into(),
        vk::LWIN => "Left Win".into(),
        vk::RWIN => "Right Win".into(),
        vk::INSERT => "Insert".into(),
        vk::FN => "Fn".into(),
        0x30..=0x39 | 0x41..=0x5A => ((vk_code as u8) as char).to_string(),
        0x70..=0x87 => format!("F{}", vk_code - 0x6F),
        other => format!("Key 0x{other:02X}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub push_to_talk: ChordSpec,
    pub hands_free: ChordSpec,
    pub command: ChordSpec,
    pub undo_ai: ChordSpec,
    pub paste_last: ChordSpec,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            push_to_talk: ChordSpec::new(mods::CTRL, vk::SPACE),
            hands_free: ChordSpec::new(mods::CTRL | mods::SHIFT, vk::SPACE),
            command: ChordSpec::new(mods::CTRL | mods::ALT, vk::SPACE),
            undo_ai: ChordSpec::new(mods::CTRL | mods::ALT, vk::KEY_Z),
            paste_last: ChordSpec::new(mods::CTRL | mods::ALT, vk::KEY_V),
        }
    }
}

impl Hotkeys {
    /// Returns pairs of chord names that are bound to the same chord.
    pub fn conflicts(&self) -> Vec<(&'static str, &'static str)> {
        let all = [
            ("push_to_talk", self.push_to_talk),
            ("hands_free", self.hands_free),
            ("command", self.command),
            ("undo_ai", self.undo_ai),
            ("paste_last", self.paste_last),
        ];
        let mut out = Vec::new();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                if all[i].1 == all[j].1 {
                    out.push((all[i].0, all[j].0));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_labels() {
        let h = Hotkeys::default();
        assert_eq!(h.push_to_talk.label_for(false), "Ctrl + Space");
        assert_eq!(h.hands_free.label_for(false), "Ctrl + Shift + Space");
        assert_eq!(h.command.label_for(false), "Ctrl + Alt + Space");
        assert!(h.conflicts().is_empty());
    }

    #[test]
    fn mac_labels() {
        let h = Hotkeys::default();
        assert_eq!(h.push_to_talk.label_for(true), "Control + Space");
        assert_eq!(h.command.label_for(true), "Control + Option + Space");
        assert_eq!(ChordSpec::new(mods::WIN, vk::KEY_V).label_for(true), "Command + V");
        assert_eq!(ChordSpec::new(0, vk::RMENU).label_for(true), "Right Option");
        assert_eq!(ChordSpec::new(0, vk::FN).label_for(true), "Fn");
    }

    #[test]
    fn modifier_only() {
        assert!(ChordSpec::new(0, vk::RCONTROL).is_modifier_only());
        assert!(ChordSpec::new(0, vk::CAPITAL).is_modifier_only());
        assert!(!ChordSpec::new(mods::CTRL, vk::SPACE).is_modifier_only());
    }
}
