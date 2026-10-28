//! What this computer is, as far as choosing a download goes: the processor, and which graphics
//! cards it has. The values only; `engine-probe` reads them from the kernel's own files
//! (`/sys/class/drm`, the NVIDIA module's version), never from a name this machine happens to
//! carry. The person may force a backend.

/// The processor family a build is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    /// 64-bit x86.
    X86_64,
    /// 64-bit ARM.
    Aarch64,
}

impl Arch {
    /// The architecture this program was built for; `None` for one no engine is built for.
    pub fn current() -> Option<Self> {
        if cfg!(target_arch = "x86_64") {
            Some(Self::X86_64)
        } else if cfg!(target_arch = "aarch64") {
            Some(Self::Aarch64)
        } else {
            None
        }
    }
}

/// What an engine's build computes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// The processor alone.
    Cpu,
    /// Any graphics card with a Vulkan driver.
    Vulkan,
    /// NVIDIA's CUDA.
    Cuda,
    /// AMD's ROCm.
    Rocm,
}

impl Backend {
    /// The name in a directory, a request and a setting.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
            Self::Cuda => "cuda",
            Self::Rocm => "rocm",
        }
    }

    /// The backend a slug names; `None` for any other text.
    pub fn from_slug(text: &str) -> Option<Self> {
        [Self::Cpu, Self::Vulkan, Self::Cuda, Self::Rocm]
            .into_iter()
            .find(|backend| backend.slug() == text)
    }
}

/// Who made a graphics card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    /// NVIDIA (PCI vendor 0x10de).
    Nvidia,
    /// AMD (0x1002).
    Amd,
    /// Intel (0x8086).
    Intel,
    /// Any other: a virtual card, say, that no engine build is for.
    Other,
}

impl GpuVendor {
    /// The vendor a PCI vendor id names (`0x10de`); `Other` for any other text.
    pub fn of_pci(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "0x10de" => Self::Nvidia,
            "0x1002" => Self::Amd,
            "0x8086" => Self::Intel,
            _ => Self::Other,
        }
    }
}

/// The graphics cards of this computer, and the NVIDIA driver's major version when it is loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gpus {
    /// One entry per card the kernel lists.
    pub vendors: Vec<GpuVendor>,
    /// The loaded NVIDIA driver's major version (`580` for `580.126.09`).
    pub nvidia_driver: Option<u32>,
}

impl Gpus {
    fn has(&self, vendor: GpuVendor) -> bool {
        self.vendors.contains(&vendor)
    }
}

/// Whether the person forced a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendChoice {
    /// Pick by the cards found.
    #[default]
    Detect,
    /// Use this one, whatever is found.
    Force(Backend),
}

/// The backends to try, best first. A forced backend is the only one; otherwise NVIDIA cards
/// prefer CUDA, any card Vulkan, and the processor alone is the last resort. ROCm is only ever
/// forced: it wants a matching system runtime that cannot be told from here.
pub fn preference(gpus: &Gpus, choice: BackendChoice) -> Vec<Backend> {
    match choice {
        BackendChoice::Force(backend) => vec![backend],
        BackendChoice::Detect => {
            let mut order = Vec::new();
            if gpus.has(GpuVendor::Nvidia) {
                order.push(Backend::Cuda);
            }
            if gpus.has(GpuVendor::Nvidia) || gpus.has(GpuVendor::Amd) || gpus.has(GpuVendor::Intel)
            {
                order.push(Backend::Vulkan);
            }
            order.push(Backend::Cpu);
            order
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backend_order_follows_the_cards_and_a_forced_one_stands_alone() {
        let gpus = |vendors: &[GpuVendor]| Gpus {
            vendors: vendors.to_vec(),
            nvidia_driver: None,
        };
        let cases = [
            (gpus(&[]), vec![Backend::Cpu]),
            (gpus(&[GpuVendor::Other]), vec![Backend::Cpu]),
            (gpus(&[GpuVendor::Amd]), vec![Backend::Vulkan, Backend::Cpu]),
            (
                gpus(&[GpuVendor::Amd, GpuVendor::Nvidia]),
                vec![Backend::Cuda, Backend::Vulkan, Backend::Cpu],
            ),
        ];
        for (found, expected) in cases {
            assert_eq!(preference(&found, BackendChoice::Detect), expected);
        }
        assert_eq!(
            preference(&gpus(&[]), BackendChoice::Force(Backend::Rocm)),
            [Backend::Rocm]
        );
        assert_eq!(Backend::from_slug("vulkan"), Some(Backend::Vulkan));
        assert_eq!(Backend::from_slug("Vulkan"), None);
    }
}
