//! Reading what this computer is: the cards from the kernel's own files (`/sys/class/drm`, the
//! NVIDIA module's version), never from a name this machine happens to carry, and the distro
//! from `os-release`. Where `/` is comes in as an argument, so a test reads its own directory.

use engine_manifest::{Arch, BackendChoice, GpuVendor, Gpus, Host, OsRelease};
use std::path::Path;

/// Reads `drm` (`/sys/class/drm`) for the cards and `nvidia_version` (the module's `version`
/// file under `/sys/module/nvidia`) for the driver. A file that is not there is no card and no
/// driver.
pub fn read_gpus(drm: &Path, nvidia_version: &Path) -> Gpus {
    let mut names: Vec<String> = std::fs::read_dir(drm)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_card(name))
        .collect();
    names.sort();
    let vendors = names
        .iter()
        .filter_map(|name| std::fs::read_to_string(drm.join(name).join("device/vendor")).ok())
        .map(|text| GpuVendor::of_pci(&text))
        .collect();
    let nvidia_driver = std::fs::read_to_string(nvidia_version)
        .ok()
        .and_then(|text| text.split('.').next()?.trim().parse().ok());
    Gpus {
        vendors,
        nvidia_driver,
    }
}

/// `card0`, `card1`: a card, not one of its connectors (`card0-HDMI-A-1`) or a render node.
fn is_card(name: &str) -> bool {
    name.strip_prefix("card")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// The computer whose files are under `root` (`/` for this one; a test's own directory
/// otherwise): its processor is this program's, its cards are what `<root>/sys/class/drm`
/// lists, its distro is `<root>/etc/os-release`. `None` for a processor no engine is built for.
pub fn read_host(root: &Path, choice: BackendChoice) -> Option<Host> {
    let under = |path: &str| root.join(path.trim_start_matches('/'));
    Some(Host {
        arch: Arch::current()?,
        gpus: read_gpus(
            &under("/sys/class/drm"),
            &under("/sys/module/nvidia/version"),
        ),
        release: OsRelease::parse(
            &std::fs::read_to_string(under("/etc/os-release")).unwrap_or_default(),
        ),
        choice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(drm: &Path, name: &str, vendor: &str) {
        let dir = drm.join(name).join("device");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("vendor"), format!("{vendor}\n")).expect("vendor");
    }

    #[test]
    fn cards_are_read_from_the_kernels_files_and_connectors_are_not_cards() {
        let dir = tempfile::tempdir().expect("scratch dir");
        card(dir.path(), "card0", "0x1002");
        card(dir.path(), "card1", "0x10de");
        card(dir.path(), "card0-HDMI-A-1", "0x8086");
        card(dir.path(), "renderD128", "0x8086");
        let version = dir.path().join("version");
        std::fs::write(&version, "580.126.09\n").expect("version");
        let gpus = read_gpus(dir.path(), &version);
        assert_eq!(gpus.vendors, [GpuVendor::Amd, GpuVendor::Nvidia]);
        assert_eq!(gpus.nvidia_driver, Some(580));
    }

    #[test]
    fn nothing_listed_is_no_card_and_no_driver() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let gpus = read_gpus(&dir.path().join("gone"), &dir.path().join("none"));
        assert_eq!(gpus, Gpus::default());
    }

    #[test]
    fn a_host_is_read_from_the_files_under_its_root() {
        let dir = tempfile::tempdir().expect("scratch dir");
        card(&dir.path().join("sys/class/drm"), "card0", "0x10de");
        std::fs::create_dir_all(dir.path().join("etc")).expect("etc");
        std::fs::write(
            dir.path().join("etc/os-release"),
            "ID=fedora\nVERSION_ID=43\n",
        )
        .expect("os-release");
        let host = read_host(dir.path(), BackendChoice::Detect).expect("a supported processor");
        assert_eq!(host.gpus.vendors, [GpuVendor::Nvidia]);
        assert_eq!(host.release.id, "fedora");
    }
}
