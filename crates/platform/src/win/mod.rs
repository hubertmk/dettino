//! Windows implementation.

mod alert;
mod authenticode;
mod autostart;
mod clipboard;
mod focus;
mod gpu;
mod memory;
mod registry;
mod sound;
mod theme;
mod uia;

pub use alert::alert;
pub use authenticode::verify_authenticode;
pub use autostart::{launch_at_login_active, set_launch_at_login, uptime};
pub use clipboard::{copy_selection, paste, read_clipboard_text, set_clipboard_text};
pub use focus::{foreground, raise_topmost, self_is_elevated};
pub use gpu::gpus;
pub use memory::{free_disk_mb, total_ram_mb};
pub use registry::hkcu_string;
pub use sound::play_sound;
pub use theme::{accent_color, taskbar_is_light};
pub use uia::{capture_context, read_focused_text, select_recent_text};

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
