//! macOS implementation: Accessibility for context and focus, NSPasteboard for
//! paste, CGEvent (through app-hotkey) for key injection.

mod ax;
mod clipboard;
mod focus;
mod system;

pub use ax::{capture_context, read_focused_text, select_recent_text};
pub use clipboard::{copy_selection, paste, read_clipboard_text, set_clipboard_text};
pub use focus::{alert, bundle_id_for, foreground, raise_topmost, self_is_elevated};
pub use system::{
    accent_color, free_disk_mb, gpus, hkcu_string, launch_at_login_active, permissions, play_sound, request_permission,
    set_launch_at_login, taskbar_is_light, total_ram_mb, uptime,
};
