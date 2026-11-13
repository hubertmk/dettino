//! Paste through `NSPasteboard` with verification, snapshot and restore.
//!
//! Our text goes on the pasteboard as a promise: an `NSPasteboardItem` whose data
//! provider is asked for the bytes only when an app reads them. The provider being
//! called means the target actually took our text, the macOS equivalent of
//! `WM_RENDERFORMAT` on Windows. AppKit calls the provider on the main thread,
//! which stays free while the paste waits on a worker thread.

use crate::{escape_html, PasteChord, PasteOutcome, PasteRequest};
use app_core::keys::{mods, vk};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AllocAnyThread};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardItemDataProvider, NSPasteboardType, NSPasteboardTypeHTML,
    NSPasteboardTypeString, NSPasteboardWriting,
};
use objc2_foundation::{NSArray, NSData, NSObject, NSObjectProtocol, NSString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SNAPSHOT_CAP: usize = 16 * 1024 * 1024;
/// nspasteboard.org marker: clipboard managers do not record items that carry it.
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

static RENDERED: AtomicBool = AtomicBool::new(false);
/// Text currently promised on the pasteboard.
static PENDING: Mutex<Option<String>> = Mutex::new(None);
static PROVIDER: Mutex<Option<ProviderRef>> = Mutex::new(None);

struct ProviderRef(Retained<Provider>);
// SAFETY: the provider has no state; AppKit may message it from any thread.
unsafe impl Send for ProviderRef {}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and Provider does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "DettinoPasteProvider"]
    struct Provider;

    unsafe impl NSObjectProtocol for Provider {}

    unsafe impl NSPasteboardItemDataProvider for Provider {
        #[unsafe(method(pasteboard:item:provideDataForType:))]
        fn provide_data(&self, _pb: Option<&NSPasteboard>, item: &NSPasteboardItem, ty: &NSPasteboardType) {
            provide(item, ty);
        }
    }
);

impl Provider {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn string_type() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeString }
}

fn html_type() -> &'static NSPasteboardType {
    unsafe { NSPasteboardTypeHTML }
}

fn provide(item: &NSPasteboardItem, ty: &NSPasteboardType) {
    let Some(text) = PENDING.lock().ok().and_then(|g| g.clone()) else { return };
    let data = if ty == string_type() {
        text
    } else if ty == html_type() {
        format!("<p>{}</p>", escape_html(&text))
    } else {
        return;
    };
    item.setString_forType(&NSString::from_str(&data), ty);
    RENDERED.store(true, Ordering::SeqCst);
}

fn general() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

fn write_items(pb: &NSPasteboard, items: Vec<Retained<NSPasteboardItem>>) -> bool {
    let objs: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items.into_iter().map(ProtocolObject::from_retained).collect();
    pb.writeObjects(&NSArray::from_retained_slice(&objs))
}

struct Snapshot {
    items: Vec<Vec<(Retained<NSString>, Vec<u8>)>>,
    partial: bool,
}

fn snapshot() -> Snapshot {
    let mut items = Vec::new();
    let mut partial = false;
    let mut total = 0usize;
    for item in general().pasteboardItems().map(|a| a.to_vec()).unwrap_or_default() {
        let mut entry = Vec::new();
        for ty in item.types().to_vec() {
            let Some(data) = item.dataForType(&ty) else {
                partial = true;
                continue;
            };
            let bytes = data.to_vec();
            if total + bytes.len() > SNAPSHOT_CAP {
                partial = true;
                continue;
            }
            total += bytes.len();
            entry.push((ty, bytes));
        }
        if !entry.is_empty() {
            items.push(entry);
        }
    }
    Snapshot { items, partial }
}

fn restore(snap: &Snapshot) -> bool {
    let pb = general();
    pb.clearContents();
    let items = snap
        .items
        .iter()
        .map(|entry| {
            let item = NSPasteboardItem::new();
            for (ty, bytes) in entry {
                item.setData_forType(&NSData::with_bytes(bytes), ty);
            }
            item
        })
        .collect();
    write_items(&pb, items)
}

/// Put text on the pasteboard right away (no promise).
fn write_immediate(text: &str) -> bool {
    if let Ok(mut g) = PENDING.lock() {
        *g = None;
    }
    let pb = general();
    pb.clearContents();
    pb.setString_forType(&NSString::from_str(text), string_type())
}

/// Promise text (plain and HTML) on the pasteboard. Returns the change count after writing.
fn write_promised(text: &str, transient: bool) -> Option<isize> {
    *PENDING.lock().ok()? = Some(text.to_string());
    RENDERED.store(false, Ordering::SeqCst);
    let provider = {
        let mut g = PROVIDER.lock().ok()?;
        g.get_or_insert_with(|| ProviderRef(Provider::new())).0.clone()
    };
    let item = NSPasteboardItem::new();
    let types = NSArray::from_slice(&[string_type(), html_type()]);
    if !item.setDataProvider_forTypes(ProtocolObject::from_ref(&*provider), &types) {
        return None;
    }
    if transient {
        item.setString_forType(&NSString::from_str(""), &NSString::from_str(TRANSIENT_TYPE));
    }
    let pb = general();
    pb.clearContents();
    write_items(&pb, vec![item]).then(|| pb.changeCount())
}

pub fn read_clipboard_text() -> Option<String> {
    general().stringForType(string_type()).map(|s| s.to_string())
}

pub fn set_clipboard_text(text: &str) -> bool {
    write_immediate(text)
}

/// The settings name Windows chords; on macOS they mean the Command equivalents.
fn send_paste_chord(chord: PasteChord) -> bool {
    match chord {
        // "Paste and Match Style"
        PasteChord::CtrlShiftV => app_hotkey::send_chord(mods::WIN | mods::ALT | mods::SHIFT, vk::KEY_V),
        PasteChord::CtrlV | PasteChord::ShiftInsert => app_hotkey::send_chord(mods::WIN, vk::KEY_V),
    }
}

/// Paste text into the focused app. Never loses the text: on any failure it is
/// left on the clipboard.
pub fn paste(req: &PasteRequest) -> PasteOutcome {
    if req.type_instead {
        return if app_hotkey::send_unicode(&req.text) { PasteOutcome::Typed } else { PasteOutcome::Failed("typing failed".into()) };
    }
    if !super::ax::trusted() {
        write_immediate(&req.text);
        return PasteOutcome::Failed("Dettino needs the Accessibility permission to paste".into());
    }
    let snap = if req.restore_clipboard { Some(snapshot()) } else { None };
    let Some(count_after) = write_promised(&req.text, req.restore_clipboard) else {
        write_immediate(&req.text);
        return PasteOutcome::Failed("Clipboard is busy".into());
    };
    if !send_paste_chord(req.chord) {
        write_immediate(&req.text);
        return PasteOutcome::Failed("Keyboard input was blocked".into());
    }
    let deadline = Instant::now() + Duration::from_millis(1000);
    while Instant::now() < deadline && !RENDERED.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(5));
    }
    if !RENDERED.load(Ordering::SeqCst) {
        // The target never read the pasteboard. Make the data concrete so a manual
        // ⌘V still works after the promise is gone.
        write_immediate(&req.text);
        return PasteOutcome::Unverified;
    }
    if !req.restore_clipboard {
        // Keep the text, but as plain data rather than a promise that ends with this
        // process. Wait until the target is done reading the other types first.
        let (text, delay) = (req.text.clone(), req.restore_delay);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if general().changeCount() == count_after {
                write_immediate(&text);
            }
        });
        return PasteOutcome::Verified { restored: None };
    }
    // Let the target finish reading other types, then restore if nobody else wrote.
    std::thread::sleep(req.restore_delay);
    let unchanged = general().changeCount() == count_after;
    let restored = match snap {
        Some(s) if unchanged && !s.items.is_empty() => restore(&s) && !s.partial,
        Some(s) if unchanged && s.items.is_empty() => {
            // Clipboard was empty before: leave our text, as plain data.
            write_immediate(&req.text);
            true
        }
        _ => false,
    };
    if let Ok(mut g) = PENDING.lock() {
        *g = None;
    }
    PasteOutcome::Verified { restored: Some(restored) }
}

/// Copy the current selection via ⌘C, then restore the clipboard.
pub fn copy_selection() -> Option<String> {
    let snap = snapshot();
    let before = general().changeCount();
    app_hotkey::send_chord(mods::WIN, vk::KEY_C);
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline && general().changeCount() == before {
        std::thread::sleep(Duration::from_millis(10));
    }
    let changed = general().changeCount() != before;
    let text = if changed { read_clipboard_text() } else { None };
    if changed {
        restore(&snap);
    }
    text.filter(|t| !t.is_empty())
}
