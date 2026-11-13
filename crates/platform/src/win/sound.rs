//! Short, quiet cues generated in memory (no sound files to ship).

use crate::tones::wav;
use crate::SoundKind;
use app_core::SoundStyle;
use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

type SoundCache = Option<HashMap<(u8, u8, u8), &'static [u8]>>;
static CACHE: Mutex<SoundCache> = Mutex::new(None);

pub fn play_sound(kind: SoundKind, style: SoundStyle, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let key = (kind as u8, style as u8, (volume.clamp(0.0, 1.0) * 20.0) as u8);
    let buf: &'static [u8] = {
        let mut g = CACHE.lock().unwrap();
        let map = g.get_or_insert_with(HashMap::new);
        map.entry(key).or_insert_with(|| Box::leak(wav(kind, style, volume).into_boxed_slice()))
    };
    unsafe {
        let _ = PlaySoundW(PCWSTR(buf.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
