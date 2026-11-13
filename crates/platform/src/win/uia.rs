//! Screen context through UI Automation, on a worker thread with a time budget.
#![allow(non_upper_case_globals)]
//! A stuck UIA call never blocks dictation: the caller stops waiting and the
//! worker is replaced.

use crate::{numeric_only, ContextRequest, ContextSnapshot};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::{Interface, BOOL, BSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

enum Job {
    Capture(ContextRequest, Instant, Sender<ContextSnapshot>),
    ReadFocused(Sender<Option<String>>),
    Select(String, Sender<bool>),
}

static WORKER: Mutex<Option<Sender<Job>>> = Mutex::new(None);

fn worker() -> Sender<Job> {
    let mut g = WORKER.lock().unwrap();
    if let Some(tx) = g.as_ref() {
        return tx.clone();
    }
    let (tx, rx) = unbounded::<Job>();
    std::thread::Builder::new().name("uia-context".into()).spawn(move || run(rx)).ok();
    *g = Some(tx.clone());
    tx
}

/// Drop the current worker (it is stuck); a fresh one starts on the next request.
fn replace_worker() {
    tracing::warn!("UI Automation call exceeded its budget; replacing worker");
    *WORKER.lock().unwrap() = None;
}

fn run(rx: Receiver<Job>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let uia: Option<IUIAutomation> = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok() };
    let Some(uia) = uia else {
        tracing::error!("UI Automation unavailable");
        return;
    };
    for job in rx {
        match job {
            Job::Capture(req, deadline, reply) => {
                let _ = reply.send(capture(&uia, req, deadline));
            }
            Job::ReadFocused(reply) => {
                let _ = reply.send(read_focused(&uia));
            }
            Job::Select(text, reply) => {
                let _ = reply.send(select(&uia, &text));
            }
        }
    }
}

fn bstr(b: windows::core::Result<BSTR>) -> Option<String> {
    b.ok().map(|s| s.to_string()).filter(|s| !s.is_empty())
}

fn control_name(id: UIA_CONTROLTYPE_ID) -> &'static str {
    match id {
        UIA_EditControlTypeId => "edit",
        UIA_DocumentControlTypeId => "document",
        UIA_ComboBoxControlTypeId => "combobox",
        UIA_TextControlTypeId => "text",
        UIA_PaneControlTypeId => "pane",
        UIA_WindowControlTypeId => "window",
        UIA_CustomControlTypeId => "custom",
        UIA_GroupControlTypeId => "group",
        _ => "other",
    }
}

fn focused(uia: &IUIAutomation) -> Option<IUIAutomationElement> {
    let el = unsafe { uia.GetFocusedElement().ok()? };
    // Never inspect our own windows (re-entrancy).
    let pid = unsafe { el.CurrentProcessId().ok()? };
    if pid as u32 == unsafe { GetCurrentProcessId() } {
        return None;
    }
    Some(el)
}

fn is_address_bar(el: &IUIAutomationElement) -> bool {
    let ctl = unsafe { el.CurrentControlType().unwrap_or_default() };
    if ctl != UIA_EditControlTypeId && ctl != UIA_ComboBoxControlTypeId {
        return false;
    }
    let id = unsafe { bstr(el.CurrentAutomationId()) }.unwrap_or_default().to_lowercase();
    let class = unsafe { bstr(el.CurrentClassName()) }.unwrap_or_default().to_lowercase();
    let name = unsafe { bstr(el.CurrentName()) }.unwrap_or_default().to_lowercase();
    address_bar_signature(&id, &class, &name)
}

/// Chromium's omnibox view is the `OmniboxViewViews` class and Firefox's URL field has
/// the automation id `urlbar-input`; those do not change with the UI language. The
/// accessible name is localized and only a last resort (English and German known).
pub(crate) fn address_bar_signature(id: &str, class: &str, name: &str) -> bool {
    class.contains("omnibox")
        || id.contains("omnibox")
        || id.contains("urlbar")
        || id.contains("address")
        || [
            "address and search bar",
            "search or enter address",
            "enter address",
            "address bar",
            "adress- und suchleiste",
            "adresse eingeben",
            "adressleiste",
        ]
        .iter()
        .any(|k| name.contains(k))
}

fn capture(uia: &IUIAutomation, req: ContextRequest, deadline: Instant) -> ContextSnapshot {
    let t0 = Instant::now();
    let mut snap = ContextSnapshot::default();
    let Some(el) = focused(uia) else {
        snap.ms = t0.elapsed().as_millis() as u64;
        return snap;
    };
    unsafe {
        snap.control = Some(control_name(el.CurrentControlType().unwrap_or_default()).into());
        snap.is_password = el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false);
    }
    snap.is_address_bar = is_address_bar(&el);
    if snap.is_password || snap.is_address_bar {
        snap.ms = t0.elapsed().as_millis() as u64;
        return snap;
    }

    if req.caret_text || req.selection {
        unsafe {
            if let Ok(tp2) = el.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) {
                let mut active = BOOL(0);
                if let Ok(caret) = tp2.GetCaretRange(&mut active) {
                    if req.caret_text {
                        if let Ok(b) = caret.Clone() {
                            let _ = b.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -300);
                            snap.before = bstr(b.GetText(-1));
                        }
                        if let Ok(a) = caret.Clone() {
                            let _ = a.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 100);
                            snap.after = bstr(a.GetText(-1));
                        }
                    }
                }
            }
            if req.selection {
                if let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                    if let Ok(arr) = tp.GetSelection() {
                        if arr.Length().unwrap_or(0) > 0 {
                            if let Ok(r) = arr.GetElement(0) {
                                snap.selection = bstr(r.GetText(2000));
                            }
                        }
                    }
                }
            }
            if snap.before.is_none() && req.caret_text {
                if let Ok(vp) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                    if let Some(v) = bstr(vp.CurrentValue()) {
                        if !numeric_only(&v) {
                            let chars: Vec<char> = v.chars().collect();
                            let start = chars.len().saturating_sub(300);
                            snap.before = Some(chars[start..].iter().collect());
                        }
                    }
                }
            }
        }
    }

    if req.visible_text && Instant::now() < deadline {
        let (text, url) = visible(uia, deadline);
        snap.visible_text = text;
        snap.url = url;
    }
    snap.ms = t0.elapsed().as_millis() as u64;
    snap
}

fn visible(uia: &IUIAutomation, deadline: Instant) -> (Option<String>, Option<String>) {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        let Ok(root) = uia.ElementFromHandle(hwnd) else { return (None, None) };
        let Ok(walker) = uia.ContentViewWalker() else { return (None, None) };
        let mut out = String::new();
        let mut url = None;
        let mut count = 0usize;
        let mut stack: Vec<(IUIAutomationElement, usize)> = vec![(root, 0)];
        while let Some((el, depth)) = stack.pop() {
            if count >= 300 || Instant::now() >= deadline || out.len() > 8000 {
                break;
            }
            count += 1;
            let ct = el.CurrentControlType().unwrap_or_default();
            if url.is_none() && ct == UIA_DocumentControlTypeId {
                if let Ok(vp) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                    if let Some(v) = bstr(vp.CurrentValue()) {
                        if v.starts_with("http") {
                            url = Some(v.split(['?', '#']).next().unwrap_or(&v).to_string());
                        }
                    }
                }
            }
            if matches!(
                ct,
                UIA_TextControlTypeId
                    | UIA_ListItemControlTypeId
                    | UIA_TabItemControlTypeId
                    | UIA_HyperlinkControlTypeId
                    | UIA_HeaderItemControlTypeId
                    | UIA_DataItemControlTypeId
                    | UIA_TreeItemControlTypeId
                    | UIA_TitleBarControlTypeId
            ) && !el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false)
            {
                if let Some(n) = bstr(el.CurrentName()) {
                    out.push_str(&n);
                    out.push('\n');
                }
            }
            if depth < 8 {
                let mut children = Vec::new();
                let mut child = walker.GetFirstChildElement(&el).ok();
                while let Some(c) = child {
                    if children.len() > 60 {
                        break;
                    }
                    let next = walker.GetNextSiblingElement(&c).ok();
                    children.push(c);
                    child = next;
                }
                for c in children.into_iter().rev() {
                    stack.push((c, depth + 1));
                }
            }
        }
        ((!out.is_empty()).then_some(out), url)
    }
}

fn read_focused(uia: &IUIAutomation) -> Option<String> {
    let el = focused(uia)?;
    unsafe {
        if el.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false) {
            return None;
        }
        if let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            if let Ok(r) = tp.DocumentRange() {
                if let Some(t) = bstr(r.GetText(20_000)) {
                    return Some(t);
                }
            }
        }
        if let Ok(vp) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            return bstr(vp.CurrentValue());
        }
    }
    None
}

fn select(uia: &IUIAutomation, text: &str) -> bool {
    let Some(el) = focused(uia) else { return false };
    unsafe {
        let Ok(tp) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) else { return false };
        let Ok(doc) = tp.DocumentRange() else { return false };
        let needle = BSTR::from(text);
        let Ok(found) = doc.FindText(&needle, true, false) else { return false };
        if found.as_raw().is_null() {
            return false;
        }
        if found.Select().is_err() {
            return false;
        }
        // Verify the selection is exactly what we pasted.
        if let Ok(arr) = tp.GetSelection() {
            if arr.Length().unwrap_or(0) > 0 {
                if let Ok(r) = arr.GetElement(0) {
                    return bstr(r.GetText(-1)).map(|s| s == text).unwrap_or(false);
                }
            }
        }
    }
    false
}

fn ask<T>(make: impl FnOnce(Sender<T>) -> Job, budget: Duration) -> Option<T> {
    let (tx, rx) = bounded(1);
    if worker().send(make(tx)).is_err() {
        replace_worker();
        return None;
    }
    match rx.recv_timeout(budget) {
        Ok(v) => Some(v),
        Err(_) => {
            replace_worker();
            None
        }
    }
}

pub fn capture_context(req: ContextRequest, budget: Duration) -> ContextSnapshot {
    let t0 = Instant::now();
    // The worker stops walking at `deadline`; we wait a little longer for its reply.
    let deadline = t0 + budget;
    match ask(|tx| Job::Capture(req, deadline, tx), budget + Duration::from_millis(80)) {
        Some(s) => s,
        None => ContextSnapshot { timed_out: true, ms: t0.elapsed().as_millis() as u64, ..Default::default() },
    }
}

pub fn read_focused_text(budget: Duration) -> Option<String> {
    ask(Job::ReadFocused, budget).flatten()
}

/// Select `text` in the focused field (searching backwards from the end) and
/// verify the selection matches. Used by "Undo AI edit".
pub fn select_recent_text(text: &str, budget: Duration) -> bool {
    let t = text.to_string();
    ask(|tx| Job::Select(t, tx), budget).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_returns_within_budget() {
        let t0 = Instant::now();
        let _ = capture_context(
            ContextRequest { caret_text: true, selection: true, visible_text: true },
            Duration::from_millis(150),
        );
        assert!(t0.elapsed() < Duration::from_millis(1500));
    }
}

#[cfg(test)]
mod address_bar_tests {
    use super::address_bar_signature as sig;

    #[test]
    fn browsers_in_any_language() {
        assert!(sig("", "omniboxviewviews", "adress- und suchleiste"), "German Chrome/Edge by class");
        assert!(sig("urlbar-input", "", "mit google suchen oder adresse eingeben"), "German Firefox by id");
        assert!(sig("", "", "search with google or enter address"), "current Firefox by name");
        assert!(sig("", "", "address and search bar"));
        assert!(!sig("", "", "message to jane"));
        assert!(!sig("searchbox", "", "search"));
    }
}
