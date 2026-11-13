//! Read-only registry helper (per-user hive, no admin rights).

use super::wide;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE};

/// The raw bytes of a value under HKEY_CURRENT_USER, whatever its type.
pub fn hkcu_binary(subkey: &str, value: &str) -> Option<Vec<u8>> {
    // SAFETY: plain registry reads with owned, NUL-terminated wide strings and a
    // buffer sized by the first query.
    unsafe {
        let mut key = HKEY::default();
        let sub = wide(subkey);
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), None, KEY_READ, &mut key).ok().ok()?;
        let name = wide(value);
        let mut len: u32 = 0;
        let mut ty = REG_VALUE_TYPE::default();
        let probe = RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, Some(&mut ty), None, Some(&mut len));
        let out = if probe.is_ok() && len > 0 {
            let mut buf = vec![0u8; len as usize];
            let read = RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, Some(&mut ty), Some(buf.as_mut_ptr()), Some(&mut len));
            if read.is_ok() {
                buf.truncate(len as usize);
                Some(buf)
            } else {
                None
            }
        } else {
            None
        };
        let _ = RegCloseKey(key);
        out
    }
}

/// A REG_SZ value under HKEY_CURRENT_USER, e.g. an installer's `InstallLocation`.
pub fn hkcu_string(subkey: &str, value: &str) -> Option<String> {
    let buf = hkcu_binary(subkey, value)?;
    let u16s: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let n = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
    Some(String::from_utf16_lossy(&u16s[..n]))
}
