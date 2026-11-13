//! Frontmost app, its focused window and focused element.
//!
//! `ForegroundInfo.app` is the lower-case bundle identifier (`com.apple.terminal`),
//! which is what app rules match on macOS. `hwnd` / `focus_hwnd` are the AX
//! identities of the focused window and element: same window and same field as
//! when the dictation started means the same numbers.

use super::ax::{self, El};
use crate::ForegroundInfo;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSString};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_core_graphics::{CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGGetDisplaysWithPoint};

/// Backing scale of the display that contains `p` (global coordinates, points).
fn scale_at(p: CGPoint) -> f64 {
    let mut id: CGDirectDisplayID = 0;
    let mut n = 0u32;
    unsafe { CGGetDisplaysWithPoint(p, 1, &mut id, &mut n) };
    if n == 0 {
        return 1.0;
    }
    let Some(mode) = CGDisplayCopyDisplayMode(id) else { return 1.0 };
    let (px, pt) = (CGDisplayMode::pixel_width(Some(&mode)), CGDisplayMode::width(Some(&mode)));
    if pt == 0 {
        1.0
    } else {
        px as f64 / pt as f64
    }
}

/// A window frame in points (top-left origin) as physical pixels, the way the
/// windowing layer reports monitor positions (points times that display's scale).
fn physical(frame: CGRect) -> (i32, i32, i32, i32) {
    let center = CGPoint::new(frame.origin.x + frame.size.width / 2.0, frame.origin.y + frame.size.height / 2.0);
    let s = scale_at(center);
    let (l, t) = (frame.origin.x * s, frame.origin.y * s);
    (l as i32, t as i32, (l + frame.size.width * s) as i32, (t + frame.size.height * s) as i32)
}

/// Main display bounds, used when a window frame is unknown.
fn main_display_rect() -> CGRect {
    CGDisplayBounds(objc2_core_graphics::CGMainDisplayID())
}

pub fn foreground() -> Option<ForegroundInfo> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    let name = app
        .bundleIdentifier()
        .map(|b| b.to_string())
        .or_else(|| app.executableURL().and_then(|u| u.lastPathComponent()).map(|n| n.to_string()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut info = ForegroundInfo {
        hwnd: pid as isize,
        focus_hwnd: 0,
        pid: pid as u32,
        app: name,
        name: app.localizedName().map(|n| n.to_string()).unwrap_or_default(),
        title: String::new(),
        blocked_elevated: false,
        is_self: pid as u32 == std::process::id(),
        rect: physical(main_display_rect()),
    };
    if !ax::trusted() || info.is_self {
        return Some(info);
    }
    ax::enable_app_ax(pid);
    let ax_app = El::app(pid);
    if let Some(win) = ax_app.element("AXFocusedWindow") {
        info.hwnd = win.id();
        info.title = win.string("AXTitle").unwrap_or_default();
        if let (Some(o), Some(s)) = (win.point("AXPosition"), win.size("AXSize")) {
            info.rect = physical(CGRect::new(o, s));
        }
    }
    if let Some(el) = ax_app.element("AXFocusedUIElement") {
        info.focus_hwnd = el.id();
    }
    Some(info)
}

/// Bundle identifier of an installed app given its name ("Slack"), lower-case.
/// Looks in the standard Applications folders; the name match ignores case.
pub fn bundle_id_for(app_name: &str) -> Option<String> {
    let want = format!("{}.app", app_name.trim().trim_end_matches(".app")).to_lowercase();
    let mut dirs: Vec<std::path::PathBuf> =
        ["/Applications", "/Applications/Utilities", "/System/Applications", "/System/Applications/Utilities", "/System/Library/CoreServices"]
            .map(Into::into)
            .into();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(home).join("Applications"));
    }
    let path = dirs.iter().find_map(|d| {
        std::fs::read_dir(d).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase() == want)
        })
    })?;
    let id = NSBundle::bundleWithPath(&NSString::from_str(&path.to_string_lossy()))?.bundleIdentifier()?;
    Some(id.to_string().to_ascii_lowercase())
}

/// Nothing to elevate on macOS: there is no UIPI equivalent.
pub fn self_is_elevated() -> bool {
    false
}

/// The pill is a floating panel and stays above normal windows by itself.
pub fn raise_topmost(_hwnd: isize) {}

/// No native message box yet (no NSAlert features enabled); only logged.
pub fn alert(title: &str, text: &str) {
    tracing::warn!(title, text, "no message box on macOS yet");
}

#[cfg(test)]
mod tests {
    #[test]
    fn foreground_does_not_panic() {
        let fg = super::foreground();
        if let Some(f) = fg {
            assert!(f.pid > 0);
            assert_eq!(f.app, f.app.to_ascii_lowercase());
        }
    }

    #[test]
    fn finds_bundle_ids() {
        assert_eq!(super::bundle_id_for("Finder").as_deref(), Some("com.apple.finder"));
        assert_eq!(super::bundle_id_for("No Such App 12345"), None);
    }
}
