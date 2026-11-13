//! Foreground window, process name, focused control and elevation.

use crate::ForegroundInfo;
use std::sync::OnceLock;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, SetWindowPos,
    GUITHREADINFO, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
};

/// Put a topmost window back at the top of the topmost band without activating it.
/// Other topmost windows that came forward later sit above it otherwise, and re-setting
/// the always-on-top flag through tao is a no-op once the flag is already set.
/// Async so a busy UI thread never blocks the caller.
pub fn raise_topmost(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        );
    }
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let r = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elev as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        r.ok()?;
        Some(elev.TokenIsElevated != 0)
    }
}

pub fn self_is_elevated() -> bool {
    static SELF: OnceLock<bool> = OnceLock::new();
    *SELF.get_or_init(|| unsafe { token_elevated(GetCurrentProcess()).unwrap_or(false) })
}

fn process_info(pid: u32) -> (String, Option<bool>) {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (String::new(), None);
        };
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let name = if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok() {
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            path.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase()
        } else {
            String::new()
        };
        let elevated = token_elevated(h);
        let _ = CloseHandle(h);
        (name, elevated)
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn foreground() -> Option<ForegroundInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut gti = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let focus = if GetGUIThreadInfo(tid, &mut gti).is_ok() { gti.hwndFocus.0 as isize } else { 0 };
        let (app, elevated) = process_info(pid);
        let blocked = match elevated {
            Some(true) => !self_is_elevated(),
            // Could not even query the process: most likely elevated or protected.
            None => !self_is_elevated() && app.is_empty(),
            _ => false,
        };
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        Some(ForegroundInfo {
            rect: (r.left, r.top, r.right, r.bottom),
            hwnd: hwnd.0 as isize,
            focus_hwnd: focus,
            pid,
            app,
            name: String::new(),
            title: window_title(hwnd),
            blocked_elevated: blocked,
            is_self: pid == GetCurrentProcessId(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn foreground_does_not_panic() {
        let _ = super::foreground();
        let _ = super::self_is_elevated();
    }
}
