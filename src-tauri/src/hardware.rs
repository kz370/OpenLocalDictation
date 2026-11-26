//! GPU detection (DXGI on Windows) and thread budget for the recognizer.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub logical_cores: usize,
    pub gpus: Vec<GpuInfo>,
}

impl HardwareInfo {
    pub fn has_nvidia(&self) -> bool {
        self.gpus.iter().any(|g| g.vendor == "nvidia")
    }

    pub fn nvidia_name(&self) -> Option<String> {
        self.gpus
            .iter()
            .find(|g| g.vendor == "nvidia")
            .map(|g| g.name.clone())
    }

    /// Threads for the recognizer: leave headroom for the rest of the system.
    pub fn inference_threads(&self) -> i32 {
        (self.logical_cores / 2).clamp(1, 8) as i32
    }
}

pub fn detect() -> HardwareInfo {
    HardwareInfo {
        logical_cores: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2),
        gpus: detect_gpus(),
    }
}

#[cfg(windows)]
fn detect_gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    let mut out = Vec::new();
    // SAFETY: plain COM calls; interfaces are reference counted by the bindings.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return out;
        };
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let vendor = match desc.VendorId {
                0x10DE => "nvidia",
                0x1002 | 0x1022 => "amd",
                0x8086 | 0x8087 => "intel",
                0x1414 => continue, // Basic Render Driver
                _ => "other",
            };
            let len = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            out.push(GpuInfo {
                name: String::from_utf16_lossy(&desc.Description[..len])
                    .trim()
                    .to_string(),
                vendor: vendor.to_string(),
            });
        }
    }
    out
}

#[cfg(not(windows))]
fn detect_gpus() -> Vec<GpuInfo> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_does_not_panic() {
        let hw = detect();
        assert!(hw.logical_cores >= 1);
        assert!((1..=8).contains(&hw.inference_threads()));
        assert!(!HardwareInfo::default().has_nvidia());
    }
}
