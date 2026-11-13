//! Launch at login via HKCU\Software\Microsoft\Windows\CurrentVersion\Run (no admin rights).

use super::registry::hkcu_binary;
use super::wide;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's Startup-apps switch for entries of the Run key.
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "Dettino";

/// Whether Windows will really start Dettino at login: the Run value is there and
/// Task Manager has not switched it off. `Ok(false)` names neither; the caller's
/// settings say which one was wanted.
pub fn launch_at_login_active() -> Result<bool, String> {
    let Some(cmd) = super::hkcu_string(RUN, NAME) else { return Ok(false) };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let same_exe = cmd.to_ascii_lowercase().contains(&exe.to_string_lossy().to_ascii_lowercase());
    if !same_exe {
        tracing::warn!(entry = %cmd, "the Run key points at another Dettino");
    }
    Ok(same_exe && crate::startup_approved_enabled(hkcu_binary(APPROVED, NAME).as_deref()))
}

/// Time since boot.
pub fn uptime() -> std::time::Duration {
    // SAFETY: no arguments, no pointers.
    std::time::Duration::from_millis(unsafe { windows::Win32::System::SystemInformation::GetTickCount64() })
}

pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        let sub = wide(RUN);
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), None, KEY_SET_VALUE, &mut key)
            .ok()
            .map_err(|e| e.to_string())?;
        let name = wide(NAME);
        let r = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" --autostart", exe.display());
            let data: Vec<u8> = wide(&cmd).iter().flat_map(|u| u.to_le_bytes()).collect();
            RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&data)).ok().map_err(|e| e.to_string())
        } else {
            let _ = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
            Ok(())
        };
        let _ = RegCloseKey(key);
        r
    }
}
