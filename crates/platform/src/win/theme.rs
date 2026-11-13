//! User's accent colour and taskbar theme, read from HKCU (no admin rights, no WinRT).

use super::wide;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

fn read_dword(sub: &str, value: &str) -> Option<u32> {
    let sub = wide(sub);
    let value = wide(value);
    let mut data: u32 = 0;
    let mut len = std::mem::size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut _),
            Some(&mut len),
        )
        .ok()
        .ok()?;
    }
    Some(data)
}

/// The accent colour from Settings → Personalization → Colors, as `#RRGGBB`.
pub fn accent_color() -> Option<String> {
    // Stored as 0xAABBGGRR.
    let v = read_dword(r"Software\Microsoft\Windows\DWM", "AccentColor")?;
    let (r, g, b) = (v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff);
    Some(format!("#{r:02X}{g:02X}{b:02X}"))
}

/// True when the taskbar and tray use the light theme (dark glyphs).
pub fn taskbar_is_light() -> bool {
    read_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "SystemUsesLightTheme") == Some(1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn accent_is_hex_when_present() {
        if let Some(c) = super::accent_color() {
            assert_eq!(c.len(), 7);
            assert!(c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit()));
        }
    }
}
