//! Tray icon and menu. The glyph shows state (ready, listening, paused, needs attention)
//! and follows the taskbar's light or dark theme.

use crate::app::{app, try_app};
use crate::controller::CtlMsg;
use crate::pill::PillState;
use crate::APP_NAME;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

static PAUSE_ITEM: Mutex<Option<MenuItem<Wry>>> = Mutex::new(None);
static UNDO_ITEM: Mutex<Option<MenuItem<Wry>>> = Mutex::new(None);
static PASTE_ITEM: Mutex<Option<MenuItem<Wry>>> = Mutex::new(None);
static TRAY: Mutex<Option<TrayIcon<Wry>>> = Mutex::new(None);
/// (glyph, light taskbar) currently shown, to skip redundant icon swaps.
static SHOWN: Mutex<Option<(Glyph, bool)>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Ready,
    Listening,
    Paused,
    Attention,
}

fn glyph_png(g: Glyph, light_taskbar: bool) -> &'static [u8] {
    match (g, light_taskbar) {
        (Glyph::Ready, false) => include_bytes!("../icons/tray/ready-dark.png"),
        (Glyph::Ready, true) => include_bytes!("../icons/tray/ready-light.png"),
        (Glyph::Listening, false) => include_bytes!("../icons/tray/listening-dark.png"),
        (Glyph::Listening, true) => include_bytes!("../icons/tray/listening-light.png"),
        (Glyph::Paused, false) => include_bytes!("../icons/tray/paused-dark.png"),
        (Glyph::Paused, true) => include_bytes!("../icons/tray/paused-light.png"),
        (Glyph::Attention, false) => include_bytes!("../icons/tray/attention-dark.png"),
        (Glyph::Attention, true) => include_bytes!("../icons/tray/attention-light.png"),
    }
}

fn glyph_for(s: &PillState) -> Glyph {
    match (s.kind.as_str(), s.badge.as_deref()) {
        ("listening", _) => Glyph::Listening,
        ("error", _) | (_, Some("error" | "no_mic" | "hotkeys_off" | "cleanup_off")) => Glyph::Attention,
        (_, Some("paused")) => Glyph::Paused,
        _ => Glyph::Ready,
    }
}

/// A menu label with its shortcut right-aligned (Windows menus split text at the tab).
fn with_chord(text: &str, chord: &str) -> String {
    if chord.is_empty() {
        text.to_string()
    } else {
        format!("{text}\t{chord}")
    }
}

pub fn create(handle: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(handle, "open", format!("Open {APP_NAME}"), true, None::<&str>)?;
    let pause = MenuItem::with_id(handle, "pause", "Pause dictation", true, None::<&str>)?;
    let undo = MenuItem::with_id(handle, "undo", "Undo AI edit", true, None::<&str>)?;
    let paste = MenuItem::with_id(handle, "paste_last", "Paste last transcript", true, None::<&str>)?;
    let history = MenuItem::with_id(handle, "history", "History", true, None::<&str>)?;
    let reset = MenuItem::with_id(handle, "reset_pill", "Reset pill position", true, None::<&str>)?;
    let quit = MenuItem::with_id(handle, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(handle)?;
    let sep2 = PredefinedMenuItem::separator(handle)?;
    let menu = Menu::with_items(handle, &[&open, &history, &sep, &pause, &undo, &paste, &reset, &sep2, &quit])?;
    *PAUSE_ITEM.lock().unwrap() = Some(pause);
    *UNDO_ITEM.lock().unwrap() = Some(undo);
    *PASTE_ITEM.lock().unwrap() = Some(paste);
    let light = app_platform::taskbar_is_light();
    let icon = Image::from_bytes(glyph_png(Glyph::Ready, light))?;
    *SHOWN.lock().unwrap() = Some((Glyph::Ready, light));
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip(format!("{APP_NAME} — hold {} to dictate", app().settings().hotkeys.push_to_talk.label()))
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app_handle, event| match event.id().as_ref() {
            "open" => crate::commands::show_main(None),
            "history" => crate::commands::show_main(Some("history".into())),
            "pause" => crate::commands::toggle_pause(),
            "undo" => {
                let _ = app().ctl.send(CtlMsg::PillAction("undo".into()));
            }
            "paste_last" => {
                std::thread::spawn(crate::pipeline::paste_last);
            }
            "reset_pill" => crate::pill::reset_position(),
            "quit" => quit_app(app_handle),
            _ => {}
        })
        .on_tray_icon_event(|_tray, event| {
            // macOS opens the menu on a left click, as every menu bar item does.
            if cfg!(not(target_os = "macos")) {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    crate::commands::show_main(None);
                }
            }
        })
        .build(handle)?;
    *TRAY.lock().unwrap() = Some(tray);
    refresh();
    Ok(())
}

/// Called on every pill state change: swap the glyph when the state or taskbar theme changed.
pub fn on_pill_state(s: &PillState) {
    let g = glyph_for(s);
    let light = app_platform::taskbar_is_light();
    let mut shown = SHOWN.lock().unwrap();
    if *shown == Some((g, light)) {
        return;
    }
    if let Some(t) = TRAY.lock().unwrap().as_ref() {
        if let Ok(img) = Image::from_bytes(glyph_png(g, light)) {
            let _ = t.set_icon(Some(img));
            *shown = Some((g, light));
        }
    }
}

pub fn refresh() {
    let Some(a) = try_app() else { return };
    let s = a.settings.read().unwrap();
    let paused = s.paused;
    let label = s.hotkeys.push_to_talk.label();
    let undo = s.hotkeys.undo_ai.label();
    let paste = s.hotkeys.paste_last.label();
    drop(s);
    if let Some(item) = PAUSE_ITEM.lock().unwrap().as_ref() {
        let _ = item.set_text(if paused { "Resume dictation" } else { "Pause dictation" });
    }
    if let Some(item) = UNDO_ITEM.lock().unwrap().as_ref() {
        let _ = item.set_text(with_chord("Undo AI edit", &undo));
    }
    if let Some(item) = PASTE_ITEM.lock().unwrap().as_ref() {
        let _ = item.set_text(with_chord("Paste last transcript", &paste));
    }
    if let Some(t) = TRAY.lock().unwrap().as_ref() {
        let _ = t.set_tooltip(Some(if paused {
            format!("{APP_NAME} — paused")
        } else {
            format!("{APP_NAME} — hold {label} to dictate")
        }));
    }
}

/// Quit for good: the tray's Quit, and an expired test build.
pub fn quit_app(app_handle: &AppHandle) {
    app_hotkey::stop();
    app().audio.shutdown();
    app_handle.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_follows_state() {
        let mut s = crate::pill::state("idle");
        assert_eq!(glyph_for(&s), Glyph::Ready);
        s.badge = Some("paused".into());
        assert_eq!(glyph_for(&s), Glyph::Paused);
        s.badge = Some("no_mic".into());
        assert_eq!(glyph_for(&s), Glyph::Attention);
        s.badge = Some("cleanup_off".into());
        assert_eq!(glyph_for(&s), Glyph::Attention);
        assert_eq!(glyph_for(&crate::pill::state("starting")), Glyph::Ready);
        assert_eq!(glyph_for(&crate::pill::state("ready")), Glyph::Ready);
        let l = crate::pill::state("listening");
        assert_eq!(glyph_for(&l), Glyph::Listening);
        assert_eq!(glyph_for(&crate::pill::state("error")), Glyph::Attention);
        assert_eq!(glyph_for(&crate::pill::state("pasted")), Glyph::Ready);
    }

    #[test]
    fn tray_pngs_decode() {
        for g in [Glyph::Ready, Glyph::Listening, Glyph::Paused, Glyph::Attention] {
            for light in [false, true] {
                assert!(Image::from_bytes(glyph_png(g, light)).is_ok());
            }
        }
    }
}
