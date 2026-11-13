//! GPU inventory via DXGI (informational: shown in Settings → Providers).

use crate::GpuInfo;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};

pub fn gpus() -> Vec<GpuInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if (desc.Flags as i32 & DXGI_ADAPTER_FLAG_SOFTWARE.0) != 0 {
                continue;
            }
            let n = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let vendor = match desc.VendorId {
                0x10DE => "NVIDIA",
                0x1002 => "AMD",
                0x8086 => "Intel",
                _ => "Other",
            };
            out.push(GpuInfo {
                name: String::from_utf16_lossy(&desc.Description[..n]),
                vram_mb: (desc.DedicatedVideoMemory / (1024 * 1024)) as u64,
                vendor: vendor.into(),
            });
        }
    }
    out
}
