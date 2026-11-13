//! Sounds, accent colour, menu bar theme, hardware, launch at login and the
//! privacy permissions Dettino needs.

use crate::tones::wav;
use crate::{GpuInfo, Permissions, SoundKind};
use app_core::SoundStyle;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{msg_send, AllocAnyThread};
use objc2_app_kit::{NSColor, NSColorSpace, NSSound};
use objc2_foundation::{NSData, NSError, NSProcessInfo, NSString, NSUserDefaults};
use std::collections::hash_map::{Entry, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ---------------------------------------------------------------- sounds

struct Sound(Retained<NSSound>);
// SAFETY: NSSound may be played and stopped from any thread.
unsafe impl Send for Sound {}

/// Keyed by (kind, style, volume step).
type SoundCache = Option<HashMap<(u8, u8, u8), Sound>>;
static SOUNDS: Mutex<SoundCache> = Mutex::new(None);

pub fn play_sound(kind: SoundKind, style: SoundStyle, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let key = (kind as u8, style as u8, (volume.clamp(0.0, 1.0) * 20.0) as u8);
    let mut g = SOUNDS.lock().unwrap();
    let s = match g.get_or_insert_with(HashMap::new).entry(key) {
        Entry::Occupied(e) => e.into_mut(),
        Entry::Vacant(e) => {
            let data = NSData::with_bytes(&wav(kind, style, volume));
            let Some(s) = NSSound::initWithData(NSSound::alloc(), &data) else { return };
            e.insert(Sound(s))
        }
    };
    if s.0.isPlaying() {
        s.0.stop();
    }
    s.0.play();
}

// ---------------------------------------------------------------- theme

/// The accent colour from System Settings → Appearance, as `#RRGGBB`.
pub fn accent_color() -> Option<String> {
    let c = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(format!("#{:02X}{:02X}{:02X}", ch(c.redComponent()), ch(c.greenComponent()), ch(c.blueComponent())))
}

/// True when the menu bar uses the light appearance (dark glyphs).
pub fn taskbar_is_light() -> bool {
    let style = NSUserDefaults::standardUserDefaults().stringForKey(&NSString::from_str("AppleInterfaceStyle"));
    style.map(|s| s.to_string() != "Dark").unwrap_or(true)
}

// ---------------------------------------------------------------- hardware

fn sysctl_string(name: &str) -> Option<String> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut len = 0usize;
    // SAFETY: size query with a null buffer, then a read into a buffer of that size.
    unsafe {
        if libc::sysctlbyname(cname.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) != 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(cname.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(len);
        Some(String::from_utf8_lossy(&buf[..end]).trim().to_string()).filter(|s| !s.is_empty())
    }
}

/// Physical memory in MB.
pub fn total_ram_mb() -> u64 {
    NSProcessInfo::processInfo().physicalMemory() / (1024 * 1024)
}

/// Free space in MB on the volume holding `path` (or its nearest existing parent);
/// `None` when it cannot be read.
pub fn free_disk_mb(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let dir = path.ancestors().find(|p| p.exists())?;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is writable.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize / (1024 * 1024))
}

/// Apple Silicon's GPU shares the system memory, so its memory is the RAM size.
pub fn gpus() -> Vec<GpuInfo> {
    if !cfg!(target_arch = "aarch64") {
        return Vec::new();
    }
    let name = sysctl_string("machdep.cpu.brand_string").unwrap_or_else(|| "Apple Silicon".into());
    vec![GpuInfo { name, vram_mb: total_ram_mb(), vendor: "Apple".into() }]
}

/// Windows registry only.
pub fn hkcu_string(_subkey: &str, _value: &str) -> Option<String> {
    None
}

// ---------------------------------------------------------------- launch at login

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// Builds before 2026-11 wrote a per-user LaunchAgent with the executable's path baked
/// in. It is removed when launch at login is set either way, so that an app moved to
/// another folder (or run through App Translocation) does not leave a dead agent behind.
const LEGACY_AGENT_LABEL: &str = "dev.dettino.desktop";

fn legacy_agent_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LEGACY_AGENT_LABEL}.plist")))
}

fn remove_legacy_agent() {
    if let Some(p) = legacy_agent_path() {
        if std::fs::remove_file(&p).is_ok() {
            tracing::info!(path = %p.display(), "removed the old LaunchAgent; login items are used now");
        }
    }
}

/// `SMAppService.mainApp` (macOS 13+): the app itself as a login item, shown under
/// System Settings → General → Login Items with its own name and icon, and tied to
/// the bundle rather than to a path.
fn main_app_service() -> Option<Retained<AnyObject>> {
    let cls = AnyClass::get(c"SMAppService")?;
    // SAFETY: a class method with no arguments returning an autoreleased object.
    unsafe { msg_send![cls, mainAppService] }
}

/// SMAppServiceStatus: notRegistered 0, enabled 1, requiresApproval 2, notFound 3.
fn service_status(svc: &AnyObject) -> isize {
    // SAFETY: a readonly NSInteger property.
    unsafe { msg_send![svc, status] }
}

/// Launch at login through the app's own login item (no admin rights, no helper app).
/// An app that is not in an Applications folder is refused: macOS would register the
/// path it runs from now, and that path is gone (disk image) or changes every launch
/// (App Translocation) by the next login.
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    remove_legacy_agent();
    let svc = main_app_service().ok_or("Login items need macOS 13 or newer")?;
    let status = service_status(&svc);
    if !enabled {
        if status == 0 || status == 3 {
            return Ok(());
        }
        // SAFETY: the method takes an NSError** out-parameter, which objc2 fills in.
        let r: Result<(), Retained<NSError>> = unsafe { msg_send![&*svc, unregisterAndReturnError: _] };
        return r.map_err(|e| e.localizedDescription().to_string());
    }
    match crate::app_location() {
        crate::AppLocation::Applications => {}
        crate::AppLocation::Translocated => {
            return Err("Move Dettino to the Applications folder first (macOS runs it from a temporary copy right now).".into())
        }
        crate::AppLocation::Elsewhere => return Err("Move Dettino to the Applications folder first, so it starts from the same place every time.".into()),
    }
    if status == 1 {
        return Ok(());
    }
    // SAFETY: as above.
    let r: Result<(), Retained<NSError>> = unsafe { msg_send![&*svc, registerAndReturnError: _] };
    r.map_err(|e| e.localizedDescription().to_string())
}

/// Whether macOS will really start Dettino at login. `Ok(false)` when it is not
/// registered; `Err` when it is registered but switched off in Login Items.
pub fn launch_at_login_active() -> Result<bool, String> {
    let Some(svc) = main_app_service() else { return Ok(false) };
    match service_status(&svc) {
        1 => Ok(true),
        2 => Err("switched off under System Settings → General → Login Items".into()),
        _ => Ok(false),
    }
}

/// Time since boot (sleep included).
pub fn uptime() -> std::time::Duration {
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    // SAFETY: `ts` is a valid out-pointer for the duration of the call.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
        return std::time::Duration::ZERO;
    }
    std::time::Duration::new(ts.tv_sec.max(0) as u64, ts.tv_nsec.clamp(0, 999_999_999) as u32)
}

// ---------------------------------------------------------------- permissions

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

/// AVMediaTypeAudio.
const MEDIA_AUDIO: &str = "soun";

fn capture_device_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

fn microphone_status() -> &'static str {
    let Some(cls) = capture_device_class() else { return "unknown" };
    let media = NSString::from_str(MEDIA_AUDIO);
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] };
    match status {
        0 => "undetermined",
        1 => "restricted",
        2 => "denied",
        3 => "granted",
        _ => "unknown",
    }
}

pub fn permissions() -> Permissions {
    Permissions {
        accessibility: super::ax::trusted(),
        input_monitoring: objc2_core_graphics::CGPreflightListenEventAccess(),
        microphone: microphone_status().into(),
    }
}

/// Ask for a permission: shows the system prompt the first time, otherwise nothing
/// (the UI then offers the matching System Settings page).
pub fn request_permission(kind: &str) {
    match kind {
        "accessibility" => {
            use objc2_application_services::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
            use objc2_core_foundation::{CFBoolean, CFDictionary};
            let key = unsafe { kAXTrustedCheckOptionPrompt };
            let opts = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
            unsafe { AXIsProcessTrustedWithOptions(Some(opts.as_opaque())) };
        }
        "input_monitoring" => {
            objc2_core_graphics::CGRequestListenEventAccess();
        }
        "microphone" => {
            let Some(cls) = capture_device_class() else { return };
            let media = NSString::from_str(MEDIA_AUDIO);
            let done = block2::RcBlock::new(|granted: Bool| tracing::info!(granted = granted.as_bool(), "microphone permission"));
            let _: () = unsafe { msg_send![cls, requestAccessForMediaType: &*media, completionHandler: &*done] };
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware() {
        assert!(total_ram_mb() > 1024);
        assert!(free_disk_mb(&std::env::temp_dir().join("does-not-exist")).is_some_and(|mb| mb > 0));
        if cfg!(target_arch = "aarch64") {
            assert!(gpus()[0].name.starts_with("Apple"));
        }
    }

    #[test]
    fn legacy_agent_lives_in_the_users_launch_agents() {
        let p = legacy_agent_path().unwrap();
        assert!(p.ends_with("Library/LaunchAgents/dev.dettino.desktop.plist"), "{}", p.display());
    }

    #[test]
    fn uptime_is_known_and_plausible() {
        let u = uptime();
        assert!(u > std::time::Duration::from_secs(1), "{u:?}");
        assert!(u < std::time::Duration::from_secs(400 * 24 * 3600), "{u:?}");
    }

    #[test]
    fn login_item_status_can_be_read() {
        // A bare test binary is not a registered login item; the call itself must work.
        let r = launch_at_login_active();
        assert!(matches!(r, Ok(false) | Err(_)), "{r:?}");
    }

    #[test]
    fn permissions_query_does_not_prompt_or_panic() {
        let p = permissions();
        assert!(["granted", "denied", "undetermined", "restricted", "unknown"].contains(&p.microphone.as_str()));
    }

    #[test]
    fn accent_is_hex() {
        if let Some(c) = accent_color() {
            assert_eq!(c.len(), 7);
        }
    }
}
