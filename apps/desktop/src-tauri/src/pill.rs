//! The pill: a transparent, non-focusable, always-on-top window that is
//! click-through everywhere except over the pill itself.

use crate::app::{app, now_ms, try_app};
use app_platform::ForegroundInfo;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use app_core::PillStyle;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Logical size of the pill window for a style. Classic needs room for the icon tray above
/// the rounded pill; One line is the line, the words on it and a row of text buttons.
/// Classic.tsx / Line.tsx draw against the same numbers.
pub fn win_size(style: PillStyle) -> (f64, f64) {
    match style {
        PillStyle::Classic => (420.0, 124.0),
        PillStyle::Line => (360.0, 80.0),
    }
}

fn current_style() -> PillStyle {
    try_app().map(|a| a.settings.read().unwrap().pill_style).unwrap_or_default()
}

fn size() -> (f64, f64) {
    win_size(current_style())
}

const BOTTOM_GAP: f64 = 24.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PillAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PillState {
    /// idle | starting | ready | listening | thinking | pasted | pasted_raw | held | nothing | cancelled | error | blocked | notice
    pub kind: String,
    pub mode: String,
    pub hands_free: bool,
    pub quiet: bool,
    pub label: Option<String>,
    pub detail: Option<String>,
    pub action: Option<PillAction>,
    pub since_ms: i64,
    pub seq: u64,
    /// Idle badge: paused | error | no_mic | downloading | hotkeys_off | cleanup_off
    pub badge: Option<String>,
    pub badge_label: Option<String>,
    pub undo_until_ms: Option<i64>,
    pub sounds: bool,
    /// Which renderer the pill window mounts (settings).
    pub style: PillStyle,
    /// How a successful paste is shown (settings; One line only).
    pub success: app_core::SuccessAnimation,
    /// Hold-to-talk chord as words, e.g. "Ctrl + Space", for the hover hint.
    pub hint: String,
}

impl Default for PillState {
    fn default() -> Self {
        Self {
            kind: "idle".into(),
            mode: "dictation".into(),
            hands_free: false,
            quiet: false,
            label: None,
            detail: None,
            action: None,
            since_ms: now_ms(),
            seq: 0,
            badge: None,
            badge_label: None,
            undo_until_ms: None,
            sounds: true,
            style: PillStyle::default(),
            success: Default::default(),
            hint: app_core::keys::Hotkeys::default().push_to_talk.label(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LevelFrame {
    pub level: f32,
    pub speech: bool,
}

static STATE: Mutex<Option<PillState>> = Mutex::new(None);
static SEQ: AtomicU64 = AtomicU64::new(1);
static LEVELS: Mutex<Option<Channel<LevelFrame>>> = Mutex::new(None);
static RECT: Mutex<(f64, f64, f64, f64)> = Mutex::new((150.0, 80.0, 120.0, 40.0));
static PROGRAMMATIC_MOVE: Mutex<Option<Instant>> = Mutex::new(None);
static MONITOR_KEY: Mutex<String> = Mutex::new(String::new());
static INTERACTIVE: AtomicBool = AtomicBool::new(false);
/// A dictation is being recorded: the pill belongs to it, not to a finishing job.
static CAPTURING: AtomicBool = AtomicBool::new(false);

pub fn set_capturing(on: bool) {
    CAPTURING.store(on, Ordering::SeqCst);
}

pub fn capturing() -> bool {
    CAPTURING.load(Ordering::SeqCst)
}

pub fn current() -> PillState {
    STATE.lock().unwrap().clone().unwrap_or_default()
}

/// Replace the pill state (keeps the idle badge unless overridden). Returns the
/// sequence number of the new state. Everything happens under the state lock, so
/// two callers cannot interleave their reads, emits and writes.
pub fn set(mut s: PillState) -> u64 {
    let mut guard = STATE.lock().unwrap();
    let prev = guard.clone().unwrap_or_default();
    if s.badge.is_none() && s.kind == "idle" {
        s.badge = prev.badge.clone();
        s.badge_label = prev.badge_label.clone();
    }
    s.seq = SEQ.fetch_add(1, Ordering::SeqCst);
    s.since_ms = now_ms();
    if let Some(a) = try_app() {
        let st = a.settings.read().unwrap();
        s.sounds = st.sounds;
        s.style = st.pill_style;
        s.success = st.success_animation;
        s.hint = st.hotkeys.push_to_talk.label();
        if st.paused && s.kind == "idle" {
            s.badge = Some("paused".into());
            s.badge_label = Some("Paused — click to resume".into());
        } else if s.kind == "idle" {
            // While Dettino is getting ready, every return to idle lands on the launch line.
            if let Some(label) = crate::startup::label() {
                s.kind = "starting".into();
                s.label = Some(label);
            }
        }
        drop(st);
        let _ = a.handle.emit_to("pill", "pill", &s);
        crate::tray::on_pill_state(&s);
    }
    let seq = s.seq;
    *guard = Some(s);
    seq
}

pub fn state(kind: &str) -> PillState {
    PillState { kind: kind.into(), ..Default::default() }
}

/// Show a state, then fall back to idle after `ms` unless something newer replaced it.
/// While a dictation is being recorded the pill is the recording's; a job that
/// finishes meanwhile leaves at most an error badge and never resets it to idle.
pub fn flash(s: PillState, ms: u64) {
    if capturing() {
        tracing::info!(kind = %s.kind, label = s.label.as_deref().unwrap_or(""), "result while recording; pill kept");
        if s.kind == "error" {
            set_badge(Some("error"), s.label.as_deref());
        }
        return;
    }
    let seq = set(s);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if current().seq == seq && !capturing() {
            set(state("idle"));
        }
    });
}

pub fn set_badge(badge: Option<&str>, label: Option<&str>) {
    let mut s = current();
    s.badge = badge.map(Into::into);
    s.badge_label = label.map(Into::into);
    if s.kind == "idle" {
        set(s);
    } else {
        *STATE.lock().unwrap() = Some(s);
    }
}

/// Remove a badge if it is the one showing. A download finishing must not wipe a
/// "no microphone" or "allow Accessibility" badge that someone else put there.
pub fn clear_badge(kind: &str) {
    if current().badge.as_deref() == Some(kind) {
        set_badge(None, None);
    }
}

pub fn subscribe_levels(ch: Channel<LevelFrame>) {
    *LEVELS.lock().unwrap() = Some(ch);
}

pub fn send_level(level: f32, speech: bool) {
    if let Some(ch) = LEVELS.lock().unwrap().as_ref() {
        let _ = ch.send(LevelFrame { level, speech });
    }
}

pub fn set_rect(x: f64, y: f64, w: f64, h: f64) {
    *RECT.lock().unwrap() = (x, y, w, h);
}

pub fn create(handle: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (win_w, win_h) = size();
    let w = WebviewWindowBuilder::new(handle, "pill", WebviewUrl::App("pill.html".into()))
        .title(crate::APP_NAME)
        .inner_size(win_w, win_h)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .focusable(false)
        .visible_on_all_workspaces(true)
        .visible(false)
        .build()?;
    let _ = w.set_ignore_cursor_events(true);
    #[cfg(target_os = "macos")]
    make_panel(&w);
    Ok(w)
}

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(PillPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

/// A plain NSWindow activates the app when clicked, which takes the focus away from
/// the field the text is meant for. A non-activating panel takes clicks without
/// that, floats over normal windows and joins every Space, full-screen ones included.
#[cfg(target_os = "macos")]
fn make_panel(w: &WebviewWindow) {
    use tauri_nspanel::{CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};
    match w.to_panel::<PillPanel>() {
        Ok(p) => {
            p.set_level(PanelLevel::Floating.value());
            if let Err(e) = p.add_style_mask(StyleMask::empty().nonactivating_panel().value()) {
                tracing::warn!("pill panel style: {e:?}");
            }
            p.set_collection_behavior(
                CollectionBehavior::new().can_join_all_spaces().full_screen_auxiliary().stationary().ignores_cycle().value(),
            );
            p.set_hides_on_deactivate(false);
        }
        Err(e) => tracing::warn!("pill panel: {e}"),
    }
}

/// The pill style changed in settings: give the window the new style's size and put it
/// back at its spot. The renderer swaps itself on the next state the window receives.
pub fn apply_style(style: PillStyle) {
    let a = app();
    let Some(win) = a.handle.get_webview_window("pill") else { return };
    let (w, h) = win_size(style);
    let _ = win.set_size(LogicalSize::new(w, h));
    MONITOR_KEY.lock().unwrap().clear();
    place(None);
}

fn monitor_key(m: &tauri::Monitor) -> String {
    format!("{}@{}x{}", m.name().cloned().unwrap_or_default(), m.size().width, m.size().height)
}

/// Move the pill to the monitor of `fg` (or the primary monitor).
pub fn place(fg: Option<&ForegroundInfo>) {
    let a = app();
    let Some(win) = a.handle.get_webview_window("pill") else { return };
    let monitor = fg
        .and_then(|f| {
            let (l, t, r, b) = f.rect;
            a.handle.monitor_from_point(((l + r) / 2) as f64, ((t + b) / 2) as f64).ok().flatten()
        })
        .or_else(|| a.handle.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let key = monitor_key(&m);
    {
        let mut cur = MONITOR_KEY.lock().unwrap();
        if *cur == key && fg.is_some() {
            raise(&win); // already on this monitor
            return;
        }
        *cur = key.clone();
    }
    let scale = m.scale_factor();
    let wa = m.work_area();
    let (dx, dy) = a
        .settings
        .read()
        .unwrap()
        .pill_positions
        .get(&key)
        .map(|p| (p.dx, p.dy))
        .unwrap_or((0.0, 0.0));
    let (win_w, win_h) = size();
    let w = win_w * scale;
    let h = win_h * scale;
    let mut x = wa.position.x as f64 + (wa.size.width as f64 - w) / 2.0 + dx * scale;
    let mut y = wa.position.y as f64 + wa.size.height as f64 - h - BOTTOM_GAP * scale + dy * scale;
    // keep inside the work area
    x = x.clamp(wa.position.x as f64, wa.position.x as f64 + wa.size.width as f64 - w);
    y = y.clamp(wa.position.y as f64, wa.position.y as f64 + wa.size.height as f64 - h);
    *PROGRAMMATIC_MOVE.lock().unwrap() = Some(Instant::now());
    let _ = win.set_position(PhysicalPosition::new(x as i32, y as i32));
    let _ = win.show();
    raise(&win);
}

/// Back on top of other always-on-top windows (meeting mini-windows, picture-in-picture,
/// pinned Task Manager) that came forward since.
fn raise(win: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(h) = win.hwnd() {
        app_platform::raise_topmost(h.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = win;
}

/// Remember where the user dragged the pill, per display.
pub fn on_moved() {
    if let Some(t) = *PROGRAMMATIC_MOVE.lock().unwrap() {
        if t.elapsed() < Duration::from_millis(600) {
            return;
        }
    }
    let a = app();
    let Some(win) = a.handle.get_webview_window("pill") else { return };
    let (Ok(pos), Ok(Some(m))) = (win.outer_position(), win.current_monitor()) else { return };
    let scale = m.scale_factor();
    let wa = m.work_area();
    let (win_w, win_h) = size();
    let w = win_w * scale;
    let h = win_h * scale;
    let def_x = wa.position.x as f64 + (wa.size.width as f64 - w) / 2.0;
    let def_y = wa.position.y as f64 + wa.size.height as f64 - h - BOTTOM_GAP * scale;
    let mut dx = (pos.x as f64 - def_x) / scale;
    let dy = (pos.y as f64 - def_y) / scale;
    // magnet to the horizontal center
    if dx.abs() < 12.0 {
        dx = 0.0;
    }
    let key = monitor_key(&m);
    *MONITOR_KEY.lock().unwrap() = key.clone();
    let mut s = a.settings.write().unwrap();
    s.pill_positions.insert(key, app_core::PillPosition { dx, dy });
    let snapshot = s.clone();
    drop(s);
    let _ = app_storage::settings::save(&a.paths.settings(), &snapshot);
    let _ = a.handle.emit("settings:changed", ());
}

pub fn reset_position() {
    let a = app();
    {
        let mut s = a.settings.write().unwrap();
        s.pill_positions.clear();
        let _ = app_storage::settings::save(&a.paths.settings(), &s);
    }
    MONITOR_KEY.lock().unwrap().clear();
    place(None);
    let _ = a.handle.emit("settings:changed", ());
}

/// Click-through except over the pill: poll the cursor every 50 ms. Every 2 s it also
/// puts the pill back on top, so it does not stay buried while idle.
pub fn start_hover_thread() {
    std::thread::Builder::new()
        .name("pill-hover".into())
        .spawn(|| for tick in (0u64..40).cycle() {
            std::thread::sleep(Duration::from_millis(50));
            let Some(a) = try_app() else { continue };
            let Some(win) = a.handle.get_webview_window("pill") else { continue };
            if tick % 40 == 0 {
                raise(&win);
            }
            let (Ok(cur), Ok(pos), Ok(scale)) = (a.handle.cursor_position(), win.outer_position(), win.scale_factor()) else {
                continue;
            };
            let (rx, ry, rw, rh) = *RECT.lock().unwrap();
            let lx = (cur.x - pos.x as f64) / scale;
            let ly = (cur.y - pos.y as f64) / scale;
            let inside = lx >= rx - 4.0 && lx <= rx + rw + 4.0 && ly >= ry - 4.0 && ly <= ry + rh + 4.0;
            if inside != INTERACTIVE.load(Ordering::SeqCst) {
                INTERACTIVE.store(inside, Ordering::SeqCst);
                let _ = win.set_ignore_cursor_events(!inside);
                let _ = a.handle.emit_to("pill", "pill:hover", inside);
            }
        })
        .ok();
}
