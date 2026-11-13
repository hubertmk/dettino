//! Crash-safe WAV writer for 16 kHz mono PCM16.
//!
//! The header is written with placeholder sizes and patched on finalize. A file
//! left unfinalized by a crash is repaired from its length by [`repair`].

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SAMPLE_RATE: u32 = 16_000;

fn header(data_len: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36u32.wrapping_add(data_len)).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Placeholder marker for an unfinalized file.
const PLACEHOLDER: u32 = 0xFFFF_FFFF;

pub struct WavWriter {
    w: BufWriter<File>,
    samples: u64,
    since_flush: usize,
}

impl WavWriter {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = File::create(path)?;
        f.write_all(&header(PLACEHOLDER))?;
        Ok(Self { w: BufWriter::new(f), samples: 0, since_flush: 0 })
    }

    pub fn write(&mut self, pcm: &[i16]) -> std::io::Result<()> {
        for s in pcm {
            self.w.write_all(&s.to_le_bytes())?;
        }
        self.samples += pcm.len() as u64;
        self.since_flush += pcm.len();
        if self.since_flush >= 3200 {
            // flush every 200 ms of audio
            self.w.flush()?;
            self.since_flush = 0;
        }
        Ok(())
    }

    pub fn finalize(mut self) -> std::io::Result<u64> {
        self.w.flush()?;
        let mut f = self.w.into_inner().map_err(|e| e.into_error())?;
        let data_len = (self.samples * 2) as u32;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&header(data_len))?;
        f.sync_all()?;
        Ok(self.samples)
    }
}

/// Fix the header of a WAV left unfinalized by a crash. Returns true if repaired.
pub fn repair(path: &Path) -> std::io::Result<bool> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let mut h = [0u8; 44];
    if f.read_exact(&mut h).is_err() {
        return Ok(false);
    }
    let data_len = u32::from_le_bytes([h[40], h[41], h[42], h[43]]);
    if data_len != PLACEHOLDER {
        return Ok(false);
    }
    let len = f.metadata()?.len();
    let data = (len.saturating_sub(44) & !1) as u32;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&header(data))?;
    Ok(true)
}

/// Read a 16 kHz mono PCM16 WAV written by this module.
pub fn read(path: &Path) -> std::io::Result<Vec<i16>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < 44 {
        return Ok(Vec::new());
    }
    Ok(bytes[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect())
}

/// Encode PCM16 as an in-memory WAV (for upload to cloud providers).
pub fn encode(pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(&header((pcm.len() * 2) as u32));
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_finalize_read() {
        let dir = std::env::temp_dir().join(format!("wavtest-{}", std::process::id()));
        let p = dir.join("a.wav");
        let mut w = WavWriter::create(&p).unwrap();
        w.write(&[1, 2, 3, -4]).unwrap();
        assert_eq!(w.finalize().unwrap(), 4);
        assert_eq!(read(&p).unwrap(), vec![1, 2, 3, -4]);
        assert!(!repair(&p).unwrap());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn crash_repair() {
        let dir = std::env::temp_dir().join(format!("wavtest2-{}", std::process::id()));
        let p = dir.join("b.wav");
        {
            let mut w = WavWriter::create(&p).unwrap();
            w.write(&vec![7i16; 5000]).unwrap();
            w.w.flush().unwrap();
            std::mem::forget(w); // simulate crash: never finalized
        }
        assert!(repair(&p).unwrap());
        assert_eq!(read(&p).unwrap().len(), 5000);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn encode_header() {
        let b = encode(&[0, 1]);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(b.len(), 48);
    }
}
