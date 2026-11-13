//! Physical memory size (shown with the GPU in Settings → Providers and used to
//! recommend a local cleanup model) and free disk space (checked before downloads).

use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// Total physical RAM in MB; 0 when the call fails.
pub fn total_ram_mb() -> u64 {
    let mut st = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `st` is a properly sized, writable MEMORYSTATUSEX.
    unsafe {
        if GlobalMemoryStatusEx(&mut st).is_ok() {
            st.ullTotalPhys / (1024 * 1024)
        } else {
            0
        }
    }
}

/// Free space in MB for this user on the drive holding `path` (or its nearest existing
/// parent); `None` when it cannot be read.
pub fn free_disk_mb(path: &Path) -> Option<u64> {
    let dir = path.ancestors().find(|p| p.exists())?;
    let wide = super::wide(&dir.to_string_lossy());
    let mut free = 0u64;
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call; `free` is writable.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), None, None) }.ok()?;
    Some(free / (1024 * 1024))
}

#[cfg(test)]
mod tests {
    #[test]
    fn free_disk_space_of_a_missing_child_uses_its_parent() {
        let tmp = std::env::temp_dir();
        let here = super::free_disk_mb(&tmp).expect("temp dir is readable");
        assert!(here > 0);
        assert!(super::free_disk_mb(&tmp.join("does-not-exist").join("deeper")).is_some());
    }
}
