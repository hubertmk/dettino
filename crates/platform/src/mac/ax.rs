//! Screen context through the Accessibility API, on a worker thread with a time
//! budget. A stuck AX call never blocks dictation: the caller stops waiting and the
//! worker is replaced (and every AX message times out after [`MESSAGING_TIMEOUT`]).
//!
//! AX ranges count UTF-16 code units, like `NSString`.

use crate::{numeric_only, ContextRequest, ContextSnapshot};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFArray, CFBoolean, CFHash, CFRange, CFRetained, CFString, CFType, CFURL, CGPoint, CGSize};
use std::collections::HashSet;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Upper bound for one AX message to another app (seconds). The default is 6 s.
const MESSAGING_TIMEOUT: f32 = 0.25;

/// An accessibility element. AX elements may be used from any thread.
pub(super) struct El(CFRetained<AXUIElement>);
unsafe impl Send for El {}

pub(super) fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

impl El {
    pub fn system() -> Self {
        static ONCE: OnceLock<()> = OnceLock::new();
        let el = El(unsafe { AXUIElement::new_system_wide() });
        ONCE.get_or_init(|| {
            // On the system-wide element this sets the default for every element.
            let _ = unsafe { el.0.set_messaging_timeout(MESSAGING_TIMEOUT) };
        });
        el
    }

    pub fn app(pid: i32) -> Self {
        El(unsafe { AXUIElement::new_application(pid) })
    }

    fn raw(&self, name: &str) -> Option<CFRetained<CFType>> {
        let attr = CFString::from_str(name);
        let mut out: *const CFType = std::ptr::null();
        let err = unsafe { self.0.copy_attribute_value(&attr, NonNull::from(&mut out)) };
        if err != AXError::Success {
            return None;
        }
        NonNull::new(out as *mut CFType).map(|p| unsafe { CFRetained::from_raw(p) })
    }

    pub fn string(&self, name: &str) -> Option<String> {
        self.raw(name)?.downcast::<CFString>().ok().map(|s| s.to_string()).filter(|s| !s.is_empty())
    }

    pub fn url(&self, name: &str) -> Option<String> {
        let v = self.raw(name)?;
        match v.downcast::<CFURL>() {
            Ok(u) => Some(u.string().to_string()),
            Err(v) => v.downcast::<CFString>().ok().map(|s| s.to_string()),
        }
    }

    pub fn element(&self, name: &str) -> Option<El> {
        self.raw(name)?.downcast::<AXUIElement>().ok().map(El)
    }

    pub fn children(&self, max: usize) -> Vec<El> {
        let Some(arr) = self.raw("AXChildren").and_then(|v| v.downcast::<CFArray>().ok()) else { return Vec::new() };
        // SAFETY: AXChildren is documented as an array of AXUIElements; each item is downcast-checked.
        let arr: CFRetained<CFArray<CFType>> = unsafe { CFRetained::cast_unchecked(arr) };
        (0..arr.len().min(max)).filter_map(|i| arr.get(i)?.downcast::<AXUIElement>().ok().map(El)).collect()
    }

    fn value<T: Copy>(&self, name: &str, ty: AXValueType, mut out: T) -> Option<T> {
        let v = self.raw(name)?.downcast::<AXValue>().ok()?;
        unsafe { v.value(ty, NonNull::from(&mut out).cast()) }.then_some(out)
    }

    pub fn range(&self, name: &str) -> Option<CFRange> {
        self.value(name, AXValueType::CFRange, CFRange { location: 0, length: 0 })
    }

    pub fn point(&self, name: &str) -> Option<CGPoint> {
        self.value(name, AXValueType::CGPoint, CGPoint::new(0.0, 0.0))
    }

    pub fn size(&self, name: &str) -> Option<CGSize> {
        self.value(name, AXValueType::CGSize, CGSize::new(0.0, 0.0))
    }

    fn set(&self, name: &str, value: &CFType) -> bool {
        unsafe { self.0.set_attribute_value(&CFString::from_str(name), value) == AXError::Success }
    }

    pub fn set_range(&self, name: &str, mut r: CFRange) -> bool {
        let Some(v) = (unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&mut r).cast()) }) else { return false };
        self.set(name, &v)
    }

    pub fn set_bool(&self, name: &str, b: bool) -> bool {
        self.set(name, CFBoolean::new(b))
    }

    /// Stable identity: equal elements hash equally.
    pub fn id(&self) -> isize {
        CFHash(Some(&self.0)) as isize
    }

    pub fn pid(&self) -> Option<i32> {
        let mut pid = 0;
        (unsafe { self.0.pid(NonNull::from(&mut pid)) } == AXError::Success).then_some(pid)
    }
}

/// Chromium and Electron apps build their accessibility tree only once asked to;
/// ask each app once, on first contact.
pub(super) fn enable_app_ax(pid: i32) {
    static DONE: Mutex<Option<HashSet<i32>>> = Mutex::new(None);
    let mut g = DONE.lock().unwrap();
    if g.get_or_insert_with(HashSet::new).insert(pid) {
        let app = El::app(pid);
        let _ = app.set_bool("AXManualAccessibility", true);
    }
}

/// The focused element of the focused app, never one of our own windows.
pub(super) fn focused() -> Option<El> {
    let el = El::system().element("AXFocusedUIElement")?;
    if el.pid() == Some(std::process::id() as i32) {
        return None;
    }
    Some(el)
}

fn is_password(el: &El) -> bool {
    el.string("AXSubrole").as_deref() == Some("AXSecureTextField")
}

fn control_name(role: &str) -> &'static str {
    match role {
        "AXTextField" | "AXSearchField" => "edit",
        "AXTextArea" | "AXWebArea" => "document",
        "AXComboBox" => "combobox",
        "AXStaticText" => "text",
        "AXGroup" | "AXScrollArea" => "group",
        "AXWindow" => "window",
        _ => "other",
    }
}

fn is_address_bar(el: &El, role: &str) -> bool {
    if role != "AXTextField" && role != "AXComboBox" {
        return false;
    }
    let id = el.string("AXIdentifier").unwrap_or_default().to_lowercase();
    let desc = el.string("AXDescription").unwrap_or_default().to_lowercase();
    id.contains("address")
        || id.contains("urlbar")
        || desc.contains("address and search bar")
        || desc.contains("search or enter")
        || desc.contains("enter address")
        || desc == "address bar"
        || desc.contains("adress- und suchleiste")
        || desc.contains("adresse eingeben")
}

fn utf16_slice(units: &[u16], start: usize, end: usize) -> Option<String> {
    let (s, e) = (start.min(units.len()), end.min(units.len()));
    (s < e).then(|| String::from_utf16_lossy(&units[s..e])).filter(|t| !t.is_empty())
}

fn capture(req: ContextRequest, deadline: Instant) -> ContextSnapshot {
    let t0 = Instant::now();
    let mut snap = ContextSnapshot::default();
    let Some(el) = focused() else {
        snap.ms = t0.elapsed().as_millis() as u64;
        return snap;
    };
    if let Some(pid) = el.pid() {
        enable_app_ax(pid);
    }
    let role = el.string("AXRole").unwrap_or_default();
    snap.control = Some(control_name(&role).into());
    snap.is_password = is_password(&el);
    snap.is_address_bar = is_address_bar(&el, &role);
    if snap.is_password || snap.is_address_bar {
        snap.ms = t0.elapsed().as_millis() as u64;
        return snap;
    }

    if req.caret_text {
        if let Some(value) = el.string("AXValue").filter(|v| !numeric_only(v)) {
            let units: Vec<u16> = value.encode_utf16().collect();
            let r = el.range("AXSelectedTextRange").unwrap_or(CFRange { location: units.len() as isize, length: 0 });
            let caret = r.location.max(0) as usize;
            let sel_end = caret + r.length.max(0) as usize;
            snap.before = utf16_slice(&units, caret.saturating_sub(300), caret);
            snap.after = utf16_slice(&units, sel_end, sel_end + 100);
        }
    }
    if req.selection {
        snap.selection = el.string("AXSelectedText").map(|s| s.chars().take(2000).collect());
    }
    if req.visible_text && Instant::now() < deadline {
        let (text, url) = visible(deadline);
        snap.visible_text = text;
        snap.url = url;
    }
    snap.ms = t0.elapsed().as_millis() as u64;
    snap
}

const TEXT_ROLES: [&str; 8] =
    ["AXStaticText", "AXHeading", "AXLink", "AXCell", "AXRow", "AXTab", "AXMenuItem", "AXButton"];

fn visible(deadline: Instant) -> (Option<String>, Option<String>) {
    let Some(app) = El::system().element("AXFocusedApplication") else { return (None, None) };
    let Some(win) = app.element("AXFocusedWindow") else { return (None, None) };
    let mut out = String::new();
    let mut url = win.url("AXDocument").filter(|u| u.starts_with("http"));
    if let Some(t) = win.string("AXTitle") {
        out.push_str(&t);
        out.push('\n');
    }
    let mut count = 0usize;
    let mut stack: Vec<(El, usize)> = vec![(win, 0)];
    while let Some((el, depth)) = stack.pop() {
        if count >= 300 || Instant::now() >= deadline || out.len() > 8000 {
            break;
        }
        count += 1;
        let role = el.string("AXRole").unwrap_or_default();
        if url.is_none() && role == "AXWebArea" {
            url = el.url("AXURL").filter(|u| u.starts_with("http"));
        }
        if TEXT_ROLES.contains(&role.as_str()) && !is_password(&el) {
            if let Some(t) = el.string("AXValue").or_else(|| el.string("AXTitle")).or_else(|| el.string("AXDescription")) {
                out.push_str(&t);
                out.push('\n');
            }
        }
        if depth < 8 {
            for c in el.children(60).into_iter().rev() {
                stack.push((c, depth + 1));
            }
        }
    }
    let url = url.map(|u| u.split(['?', '#']).next().unwrap_or(&u).to_string());
    ((!out.is_empty()).then_some(out), url)
}

fn read_focused() -> Option<String> {
    let el = focused()?;
    if is_password(&el) {
        return None;
    }
    el.string("AXValue").map(|v| v.chars().take(20_000).collect())
}

/// Select the last occurrence of `text` in the focused field and verify it.
fn select(text: &str) -> bool {
    let Some(el) = focused() else { return false };
    let Some(value) = el.string("AXValue") else { return false };
    let Some(byte_pos) = value.rfind(text) else { return false };
    let location = value[..byte_pos].encode_utf16().count() as isize;
    let length = text.encode_utf16().count() as isize;
    if !el.set_range("AXSelectedTextRange", CFRange { location, length }) {
        return false;
    }
    el.string("AXSelectedText").is_some_and(|s| s == text)
}

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
    std::thread::Builder::new().name("ax-context".into()).spawn(move || run(rx)).ok();
    *g = Some(tx.clone());
    tx
}

/// Drop the current worker (it is stuck); a fresh one starts on the next request.
fn replace_worker() {
    tracing::warn!("Accessibility call exceeded its budget; replacing worker");
    *WORKER.lock().unwrap() = None;
}

fn run(rx: Receiver<Job>) {
    for job in rx {
        match job {
            Job::Capture(req, deadline, reply) => {
                let _ = reply.send(capture(req, deadline));
            }
            Job::ReadFocused(reply) => {
                let _ = reply.send(read_focused());
            }
            Job::Select(text, reply) => {
                let _ = reply.send(select(&text));
            }
        }
    }
}

fn ask<T>(make: impl FnOnce(Sender<T>) -> Job, budget: Duration) -> Option<T> {
    if !trusted() {
        return None;
    }
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
        None => ContextSnapshot { timed_out: trusted(), ms: t0.elapsed().as_millis() as u64, ..Default::default() },
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

    #[test]
    fn utf16_slices() {
        let units: Vec<u16> = "Grüße 👋 dir".encode_utf16().collect();
        assert_eq!(utf16_slice(&units, 0, 5).as_deref(), Some("Grüße"));
        assert_eq!(utf16_slice(&units, 6, 8).as_deref(), Some("👋"));
        assert_eq!(utf16_slice(&units, 50, 60), None);
    }

    #[test]
    fn roles() {
        assert_eq!(control_name("AXTextArea"), "document");
        assert_eq!(control_name("AXTextField"), "edit");
        assert_eq!(control_name("AXSlider"), "other");
    }
}
