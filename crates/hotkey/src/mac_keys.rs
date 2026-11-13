//! macOS virtual key codes (`kVK_*`, HIToolbox `Events.h`) to and from the Windows
//! virtual-key codes the tracker and settings use. Pure tables, compiled everywhere
//! so the mapping is tested on every CI runner.

use app_core::keys::vk;

/// (macOS keycode, Windows VK). Left-hand modifiers come before right-hand ones so
/// the reverse lookup of a side-independent VK finds the left key.
const TABLE: &[(u16, u16)] = &[
    // letters
    (0x00, 0x41), // A
    (0x0B, 0x42), // B
    (0x08, 0x43), // C
    (0x02, 0x44), // D
    (0x0E, 0x45), // E
    (0x03, 0x46), // F
    (0x05, 0x47), // G
    (0x04, 0x48), // H
    (0x22, 0x49), // I
    (0x26, 0x4A), // J
    (0x28, 0x4B), // K
    (0x25, 0x4C), // L
    (0x2E, 0x4D), // M
    (0x2D, 0x4E), // N
    (0x1F, 0x4F), // O
    (0x23, 0x50), // P
    (0x0C, 0x51), // Q
    (0x0F, 0x52), // R
    (0x01, 0x53), // S
    (0x11, 0x54), // T
    (0x20, 0x55), // U
    (0x09, 0x56), // V
    (0x0D, 0x57), // W
    (0x07, 0x58), // X
    (0x10, 0x59), // Y
    (0x06, 0x5A), // Z
    // digits
    (0x1D, 0x30),
    (0x12, 0x31),
    (0x13, 0x32),
    (0x14, 0x33),
    (0x15, 0x34),
    (0x17, 0x35),
    (0x16, 0x36),
    (0x1A, 0x37),
    (0x1C, 0x38),
    (0x19, 0x39),
    // punctuation (US positions)
    (0x29, 0xBA), // ;
    (0x18, 0xBB), // =
    (0x2B, 0xBC), // ,
    (0x1B, 0xBD), // -
    (0x2F, 0xBE), // .
    (0x2C, 0xBF), // /
    (0x32, 0xC0), // `
    (0x21, 0xDB), // [
    (0x2A, 0xDC), // \
    (0x1E, 0xDD), // ]
    (0x27, 0xDE), // '
    (0x0A, 0xE2), // ISO § / <> key
    // editing and navigation
    (0x24, vk::RETURN),
    (0x30, vk::TAB),
    (0x31, vk::SPACE),
    (0x33, vk::BACK),
    (0x35, vk::ESCAPE),
    (0x72, vk::INSERT), // Help sits where Insert is on PC keyboards
    (0x75, 0x2E),       // forward delete
    (0x73, 0x24),       // Home
    (0x77, 0x23),       // End
    (0x74, 0x21),       // Page Up
    (0x79, 0x22),       // Page Down
    (0x7B, vk::LEFT),
    (0x7E, 0x26), // Up
    (0x7C, 0x27), // Right
    (0x7D, 0x28), // Down
    // function keys
    (0x7A, 0x70),
    (0x78, 0x71),
    (0x63, 0x72),
    (0x76, 0x73),
    (0x60, 0x74),
    (0x61, 0x75),
    (0x62, 0x76),
    (0x64, 0x77),
    (0x65, 0x78),
    (0x6D, 0x79),
    (0x67, 0x7A),
    (0x6F, 0x7B),
    (0x69, vk::F13),
    (0x6B, 0x7D),
    (0x71, 0x7E),
    (0x6A, 0x7F),
    (0x40, 0x80),
    (0x4F, 0x81),
    (0x50, 0x82),
    (0x5A, 0x83),
    // keypad
    (0x52, 0x60),
    (0x53, 0x61),
    (0x54, 0x62),
    (0x55, 0x63),
    (0x56, 0x64),
    (0x57, 0x65),
    (0x58, 0x66),
    (0x59, 0x67),
    (0x5B, 0x68),
    (0x5C, 0x69),
    (0x43, 0x6A), // *
    (0x45, 0x6B), // +
    (0x4E, 0x6D), // -
    (0x41, 0x6E), // .
    (0x4B, 0x6F), // /
    (0x4C, vk::RETURN), // keypad Enter (after Return, so Return wins the reverse lookup)
    // modifiers
    (0x3B, vk::LCONTROL),
    (0x3E, vk::RCONTROL),
    (0x38, vk::LSHIFT),
    (0x3C, vk::RSHIFT),
    (0x3A, vk::LMENU),
    (0x3D, vk::RMENU),
    (0x37, vk::LWIN), // Command
    (0x36, vk::RWIN),
    (0x39, vk::CAPITAL),
    (0x3F, vk::FN),
];

/// Windows VK for a macOS keycode, or 0 for keys the tracker never needs.
pub fn to_vk(keycode: u16) -> u16 {
    TABLE.iter().find(|(k, _)| *k == keycode).map(|(_, v)| *v).unwrap_or(0)
}

/// macOS keycode for a Windows VK. Side-independent modifiers map to the left key.
pub fn from_vk(code: u16) -> Option<u16> {
    let code = match code {
        vk::CONTROL => vk::LCONTROL,
        vk::SHIFT => vk::LSHIFT,
        vk::MENU => vk::LMENU,
        other => other,
    };
    TABLE.iter().find(|(_, v)| *v == code).map(|(k, _)| *k)
}

/// Device-dependent modifier bits in `CGEventFlags` (IOKit `NX_DEVICE*KEYMASK`). The
/// device-independent masks cannot tell left from right, so a FlagsChanged event is
/// decoded with these: the bit for the event's own key says whether it went down.
pub fn side_mask(code: u16) -> u64 {
    match code {
        vk::LCONTROL => 0x0000_0001,
        vk::LSHIFT => 0x0000_0002,
        vk::RSHIFT => 0x0000_0004,
        vk::LWIN => 0x0000_0008,
        vk::RWIN => 0x0000_0010,
        vk::LMENU => 0x0000_0020,
        vk::RMENU => 0x0000_0040,
        vk::RCONTROL => 0x0000_2000,
        vk::FN => 0x0080_0000, // kCGEventFlagMaskSecondaryFn (no per-side bit)
        _ => 0,
    }
}

/// Device-independent `CGEventFlags` mask for a modifier bit set ([`app_core::keys::mods`]).
pub fn flags_for_mods(m: u8) -> u64 {
    use app_core::keys::mods;
    let mut f = 0;
    if m & mods::SHIFT != 0 {
        f |= 0x0002_0000;
    }
    if m & mods::CTRL != 0 {
        f |= 0x0004_0000;
    }
    if m & mods::ALT != 0 {
        f |= 0x0008_0000;
    }
    if m & mods::WIN != 0 {
        f |= 0x0010_0000;
    }
    f
}

/// [`app_core::keys::mods`] bits held according to `CGEventFlags`.
pub fn mods_from_flags(f: u64) -> u8 {
    use app_core::keys::mods;
    let mut m = 0;
    if f & 0x0002_0000 != 0 {
        m |= mods::SHIFT;
    }
    if f & 0x0004_0000 != 0 {
        m |= mods::CTRL;
    }
    if f & 0x0008_0000 != 0 {
        m |= mods::ALT;
    }
    if f & 0x0010_0000 != 0 {
        m |= mods::WIN;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::keys::mods;

    #[test]
    fn round_trips() {
        for &(k, v) in TABLE {
            assert_eq!(from_vk(v).map(to_vk), Some(v), "keycode 0x{k:02X}");
        }
        assert_eq!(to_vk(0x31), vk::SPACE);
        assert_eq!(from_vk(vk::KEY_V), Some(0x09));
        assert_eq!(from_vk(vk::RETURN), Some(0x24), "Return, not keypad Enter");
    }

    #[test]
    fn generic_modifiers_map_to_the_left_key() {
        assert_eq!(from_vk(vk::CONTROL), Some(0x3B));
        assert_eq!(from_vk(vk::SHIFT), Some(0x38));
        assert_eq!(from_vk(vk::MENU), Some(0x3A));
    }

    #[test]
    fn unknown_keys() {
        assert_eq!(to_vk(0x48), 0, "volume up is not mapped");
        assert_eq!(from_vk(0x07), None);
    }

    #[test]
    fn flags() {
        let all = mods::CTRL | mods::SHIFT | mods::ALT | mods::WIN;
        assert_eq!(mods_from_flags(flags_for_mods(all)), all);
        assert_eq!(mods_from_flags(0x0004_0000 | 0x0000_0001), mods::CTRL);
        assert_eq!(side_mask(vk::RCONTROL), 0x2000);
    }
}
