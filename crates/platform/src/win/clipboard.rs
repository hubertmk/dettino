//! Clipboard paste with delayed rendering (paste verification), snapshot and
//! restore. A message-only window on its own thread owns our clipboard data;
//! `WM_RENDERFORMAT` arriving means the target app actually read it.

use super::wide;
use crate::{escape_html, PasteChord, PasteOutcome, PasteRequest};
use app_core::keys::{mods, vk};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber,
    GetOpenClipboardWindow, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowThreadProcessId, RegisterClassW, TranslateMessage, HWND_MESSAGE, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSW,
};

const CF_UNICODETEXT: u32 = 13;
/// GDI-handle formats that cannot be copied as memory blocks.
const NON_GLOBAL: [u32; 8] = [2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E];
const SNAPSHOT_CAP: usize = 16 * 1024 * 1024;

static OWNER: AtomicIsize = AtomicIsize::new(0);
static RENDERED: AtomicBool = AtomicBool::new(false);
/// Process id of the window the paste goes to; only its clipboard read verifies the paste.
static EXPECTED_PID: AtomicU32 = AtomicU32::new(0);
static PENDING: Mutex<Option<(Vec<u16>, Vec<u8>)>> = Mutex::new(None);
static INIT: OnceLock<()> = OnceLock::new();

fn html_format() -> u32 {
    static F: OnceLock<u32> = OnceLock::new();
    *F.get_or_init(|| unsafe { RegisterClipboardFormatW(PCWSTR(wide("HTML Format").as_ptr())) })
}

/// CF_HTML blob with byte offsets (https://learn.microsoft.com/windows/win32/dataxchg/html-clipboard-format).
pub(crate) fn html_blob(text: &str) -> Vec<u8> {
    let fragment = format!("<p>{}</p>", escape_html(text));
    let pre = "<html><body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body></html>";
    let header_len = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n".len();
    let start_html = header_len;
    let start_frag = start_html + pre.len();
    let end_frag = start_frag + fragment.len();
    let end_html = end_frag + post.len();
    let mut s = format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_frag:010}\r\nEndFragment:{end_frag:010}\r\n"
    );
    s.push_str(pre);
    s.push_str(&fragment);
    s.push_str(post);
    let mut b = s.into_bytes();
    b.push(0);
    b
}

/// Formats that keep our text out of Clipboard History and the cloud clipboard.
/// Dictated text is private; it also stops those readers from "verifying" a paste.
fn exclusion_formats() -> [u32; 3] {
    static F: OnceLock<[u32; 3]> = OnceLock::new();
    *F.get_or_init(|| {
        ["ExcludeClipboardContentFromMonitorProcessing", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"]
            .map(|n| unsafe { RegisterClipboardFormatW(PCWSTR(wide(n).as_ptr())) })
    })
}

/// Set the data for a format; the handle is ours again (and freed) if that fails.
unsafe fn set_format(format: u32, bytes: &[u8]) -> bool {
    let Some(h) = global_from(bytes) else { return false };
    if SetClipboardData(format, Some(HANDLE(h.0))).is_err() {
        let _ = GlobalFree(Some(h));
        return false;
    }
    true
}

/// With the clipboard open: mark the content as not for history / cloud sync.
unsafe fn mark_private() {
    for fmt in exclusion_formats() {
        set_format(fmt, &0u32.to_le_bytes());
    }
}

/// The process that currently has the clipboard open, if it opened it with a window.
fn reader_pid() -> Option<u32> {
    unsafe {
        let w = GetOpenClipboardWindow().ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(w, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

unsafe fn global_from(bytes: &[u8]) -> Option<HGLOBAL> {
    let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).ok()?;
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(h));
        return None;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(h);
    Some(h)
}

unsafe fn render(format: u32) {
    let pending = PENDING.lock().ok().and_then(|g| g.clone());
    let Some((text, html)) = pending else { return };
    let bytes: Vec<u8> = if format == CF_UNICODETEXT {
        text.iter().flat_map(|u| u.to_le_bytes()).collect()
    } else if format == html_format() {
        html
    } else {
        return;
    };
    set_format(format, &bytes);
    // Only the target's read is a paste. Clipboard History, clipboard managers and RDP
    // read every new clipboard too; a reader that opened the clipboard without a window
    // cannot be told apart and counts, as before.
    let expected = EXPECTED_PID.load(Ordering::SeqCst);
    let reader = reader_pid();
    if expected == 0 || reader.is_none() || reader == Some(expected) {
        RENDERED.store(true, Ordering::SeqCst);
    } else {
        tracing::debug!(?reader, expected, "clipboard rendered for another process");
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_RENDERFORMAT => {
            render(wparam.0 as u32);
            LRESULT(0)
        }
        WM_RENDERALLFORMATS => {
            if OpenClipboard(Some(hwnd)).is_ok() {
                if GetClipboardOwner().ok().map(|o| o.0) == Some(hwnd.0) {
                    render(CF_UNICODETEXT);
                    render(html_format());
                }
                let _ = CloseClipboard();
            }
            LRESULT(0)
        }
        WM_DESTROYCLIPBOARD => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn owner_hwnd() -> Option<HWND> {
    INIT.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("clipboard-owner".into())
            .spawn(move || unsafe {
                let hinst = GetModuleHandleW(None).unwrap_or_default();
                let class = wide("DettinoClipboardOwner");
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinst.into(),
                    lpszClassName: PCWSTR(class.as_ptr()),
                    ..Default::default()
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    PCWSTR(class.as_ptr()),
                    PCWSTR(class.as_ptr()),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(hinst.into()),
                    None,
                );
                match hwnd {
                    Ok(h) => {
                        OWNER.store(h.0 as isize, Ordering::SeqCst);
                        let _ = tx.send(());
                        let mut msg = MSG::default();
                        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    Err(e) => {
                        tracing::error!("clipboard window failed: {e}");
                        let _ = tx.send(());
                    }
                }
            })
            .ok();
        let _ = rx.recv_timeout(Duration::from_secs(2));
    });
    let h = OWNER.load(Ordering::SeqCst);
    (h != 0).then_some(HWND(h as *mut _))
}

/// Open the clipboard, retrying while another process holds it.
fn open(hwnd: Option<HWND>) -> bool {
    for _ in 0..20 {
        if unsafe { OpenClipboard(hwnd) }.is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

struct Snapshot {
    items: Vec<(u32, Vec<u8>)>,
    partial: bool,
}

fn snapshot() -> Option<Snapshot> {
    if !open(None) {
        return None;
    }
    let mut items = Vec::new();
    let mut partial = false;
    let mut total = 0usize;
    unsafe {
        let mut fmt = 0u32;
        loop {
            fmt = EnumClipboardFormats(fmt);
            if fmt == 0 {
                break;
            }
            if NON_GLOBAL.contains(&fmt) {
                partial = true;
                continue;
            }
            let Ok(h) = GetClipboardData(fmt) else { continue };
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            if size == 0 {
                continue;
            }
            if total + size > SNAPSHOT_CAP {
                partial = true;
                continue;
            }
            let p = GlobalLock(hg) as *const u8;
            if p.is_null() {
                continue;
            }
            let data = std::slice::from_raw_parts(p, size).to_vec();
            let _ = GlobalUnlock(hg);
            total += size;
            items.push((fmt, data));
        }
        let _ = CloseClipboard();
    }
    Some(Snapshot { items, partial })
}

/// Put the snapshot back. The clipboard must be opened with our window: after
/// `OpenClipboard(NULL)`, `EmptyClipboard` sets the owner to NULL and every
/// `SetClipboardData` fails, which would leave the clipboard empty.
fn restore(snap: &Snapshot) -> bool {
    let hwnd = owner_hwnd();
    if !open(hwnd) {
        return false;
    }
    let mut ok = true;
    unsafe {
        if EmptyClipboard().is_err() {
            ok = false;
        }
        for (fmt, data) in &snap.items {
            ok &= set_format(*fmt, data);
        }
        let _ = CloseClipboard();
    }
    if let Ok(mut g) = PENDING.lock() {
        *g = None;
    }
    ok
}

/// Put text (and an HTML version) on the clipboard immediately, owned by our window.
fn write_immediate(text: &str) -> bool {
    let hwnd = owner_hwnd();
    if !open(hwnd) {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let utf16: Vec<u8> = text.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect();
        set_format(CF_UNICODETEXT, &utf16);
        mark_private();
        let _ = CloseClipboard();
    }
    if let Ok(mut g) = PENDING.lock() {
        *g = None;
    }
    true
}

/// Put text on the clipboard with delayed rendering. Returns the sequence number after writing.
fn write_delayed(text: &str) -> Option<u32> {
    let hwnd = owner_hwnd()?;
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    *PENDING.lock().ok()? = Some((utf16, html_blob(text)));
    RENDERED.store(false, Ordering::SeqCst);
    if !open(Some(hwnd)) {
        return None;
    }
    unsafe {
        let _ = EmptyClipboard();
        let _ = SetClipboardData(CF_UNICODETEXT, None);
        let _ = SetClipboardData(html_format(), None);
        mark_private();
        let _ = CloseClipboard();
        Some(GetClipboardSequenceNumber())
    }
}

pub fn read_clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return None;
        }
    }
    if !open(None) {
        return None;
    }
    let out = unsafe {
        let r = GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
            let hg = HGLOBAL(h.0);
            let p = GlobalLock(hg) as *const u16;
            if p.is_null() {
                return None;
            }
            let max = GlobalSize(hg) / 2;
            let mut n = 0;
            while n < max && *p.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
            let _ = GlobalUnlock(hg);
            Some(s)
        });
        let _ = CloseClipboard();
        r
    };
    out
}

pub fn set_clipboard_text(text: &str) -> bool {
    write_immediate(text)
}

fn send_paste_chord(chord: PasteChord) -> bool {
    match chord {
        PasteChord::CtrlV => app_hotkey::send_chord(mods::CTRL, vk::KEY_V),
        PasteChord::CtrlShiftV => app_hotkey::send_chord(mods::CTRL | mods::SHIFT, vk::KEY_V),
        PasteChord::ShiftInsert => app_hotkey::send_chord(mods::SHIFT, vk::INSERT),
    }
}

/// Paste text into the focused app. Never loses the text: on any failure it is
/// left on the clipboard.
pub fn paste(req: &PasteRequest) -> PasteOutcome {
    if req.type_instead {
        return if app_hotkey::send_unicode(&req.text) { PasteOutcome::Typed } else { PasteOutcome::Failed("typing failed".into()) };
    }
    let snap = if req.restore_clipboard { snapshot() } else { None };
    let Some(seq_after) = write_delayed(&req.text) else {
        // Fall back to a plain write so the user can paste by hand.
        write_immediate(&req.text);
        return PasteOutcome::Failed("Clipboard is busy".into());
    };
    EXPECTED_PID.store(req.target_pid.unwrap_or(0), Ordering::SeqCst);
    if !send_paste_chord(req.chord) {
        EXPECTED_PID.store(0, Ordering::SeqCst);
        write_immediate(&req.text);
        return PasteOutcome::Failed("Keyboard input was blocked".into());
    }
    let deadline = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < deadline && !RENDERED.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(5));
    }
    EXPECTED_PID.store(0, Ordering::SeqCst);
    if !RENDERED.load(Ordering::SeqCst) {
        // The target never read the clipboard. Make the data concrete so a manual
        // Ctrl+V still works after we stop owning it.
        write_immediate(&req.text);
        return PasteOutcome::Unverified;
    }
    if !req.restore_clipboard {
        return PasteOutcome::Verified { restored: None };
    }
    // Let the target finish reading other formats, then restore if nobody else wrote.
    std::thread::sleep(req.restore_delay);
    let now_seq = unsafe { GetClipboardSequenceNumber() };
    let restored = match snap {
        Some(s) if now_seq == seq_after && !s.items.is_empty() => restore(&s) && !s.partial,
        Some(s) if s.items.is_empty() => {
            // Clipboard was empty before: leave our text (harmless) but stop owning it
            // lazily, unless someone else wrote in the meantime.
            if now_seq == seq_after {
                write_immediate(&req.text);
            }
            true
        }
        _ => false,
    };
    if let Ok(mut g) = PENDING.lock() {
        *g = None;
    }
    PasteOutcome::Verified { restored: Some(restored) }
}

/// Copy the current selection via Ctrl+C, then restore the clipboard.
pub fn copy_selection() -> Option<String> {
    let snap = snapshot();
    let before = unsafe { GetClipboardSequenceNumber() };
    app_hotkey::send_chord(mods::CTRL, vk::KEY_C);
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline && unsafe { GetClipboardSequenceNumber() } == before {
        std::thread::sleep(Duration::from_millis(10));
    }
    let changed = unsafe { GetClipboardSequenceNumber() } != before;
    let text = if changed { read_clipboard_text() } else { None };
    if let Some(s) = snap {
        if changed {
            restore(&s);
        }
    }
    text.filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_offsets_are_consistent() {
        let b = html_blob("a < b\nc");
        let s = String::from_utf8(b[..b.len() - 1].to_vec()).unwrap();
        let get = |k: &str| -> usize {
            let i = s.find(k).unwrap() + k.len();
            s[i..i + 10].parse().unwrap()
        };
        let sf = get("StartFragment:");
        let ef = get("EndFragment:");
        assert_eq!(&s[sf..ef], "<p>a &lt; b<br>c</p>");
        assert_eq!(get("EndHTML:"), s.len());
    }
}
