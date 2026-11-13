//! A plain message box, for the moments the app has no window of its own yet.

use super::wide;
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST};

/// Show `text` and wait until it is dismissed.
pub fn alert(title: &str, text: &str) {
    let (title, text) = (wide(title), wide(text));
    unsafe {
        MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST);
    }
}
