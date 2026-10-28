//! Which distro this is: the `os-release` file read for the few words a package choice needs.
//! Whether it ships a package is [`OsRelease::ships`]; the packages are `engine-spec`'s.

use engine_spec::{PackageSource, Releases, Unnumbered};

/// The words of `os-release` a package choice needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsRelease {
    /// `ID`: `fedora`, `ubuntu`, `arch`.
    pub id: String,
    /// `ID_LIKE`, one word each: `debian` for Ubuntu, `arch` for Manjaro.
    pub like: Vec<String>,
    /// `VERSION_ID` as (major, minor): `26.04` is (26, 4), `43` is (43, 0). `None` for a
    /// rolling or testing release that has none.
    pub version: Option<(u32, u32)>,
}

impl OsRelease {
    /// The file's text, read leniently: a line that is not `KEY=value` is skipped.
    pub fn parse(text: &str) -> Self {
        let value = |key: &str| {
            text.lines()
                .filter_map(|line| line.split_once('='))
                .find(|(name, _)| name.trim() == key)
                .map(|(_, value)| value.trim().trim_matches(['"', '\'']).to_owned())
        };
        Self {
            id: value("ID").unwrap_or_default().to_ascii_lowercase(),
            like: value("ID_LIKE")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_ascii_lowercase)
                .collect(),
            version: value("VERSION_ID").and_then(|text| {
                let mut parts = text.split('.');
                let major = parts.next()?.parse().ok()?;
                let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
                Some((major, minor))
            }),
        }
    }
}

impl OsRelease {
    /// Whether `source` is known to exist on this release.
    pub fn ships(&self, source: &PackageSource) -> bool {
        match source.releases {
            Releases::Rolling { id } => self.id == id || self.like.iter().any(|l| l == id),
            Releases::From {
                id,
                version,
                unnumbered,
            } => {
                self.id == id
                    && match self.version {
                        Some(found) => found >= version,
                        None => unnumbered == Unnumbered::New,
                    }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_spec::PackageManager;
    use engine_spec::Verified;

    const UBUNTU_24: &str = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\n";

    fn from(id: &'static str, version: (u32, u32), unnumbered: Unnumbered) -> PackageSource {
        PackageSource {
            manager: PackageManager::Apt,
            name: "x",
            releases: Releases::From {
                id,
                version,
                unnumbered,
            },
            verified: Verified::Yes,
        }
    }

    #[test]
    fn os_release_is_read_for_id_like_and_version() {
        let release = OsRelease::parse(UBUNTU_24);
        assert_eq!(release.id, "ubuntu");
        assert_eq!(release.like, ["debian"]);
        assert_eq!(release.version, Some((24, 4)));
        assert_eq!(
            OsRelease::parse("ID=fedora\nVERSION_ID=43\n").version,
            Some((43, 0))
        );
        assert_eq!(OsRelease::parse("ID=arch\n").version, None);
        assert_eq!(OsRelease::parse(""), OsRelease::default());
    }

    #[test]
    fn a_versioned_package_exists_only_on_its_own_id_and_new_enough_releases() {
        let ubuntu_26 = from("ubuntu", (26, 4), Unnumbered::Old);
        assert!(!OsRelease::parse(UBUNTU_24).ships(&ubuntu_26));
        assert!(OsRelease::parse("ID=ubuntu\nVERSION_ID=\"26.04\"\n").ships(&ubuntu_26));
        // Ubuntu says it is like Debian; a Debian number means nothing for it.
        let debian_14 = from("debian", (14, 0), Unnumbered::New);
        assert!(!OsRelease::parse(UBUNTU_24).ships(&debian_14));
        assert!(OsRelease::parse("ID=debian\n").ships(&debian_14));
        assert!(!OsRelease::parse("ID=debian\n").ships(&from("debian", (14, 0), Unnumbered::Old)));
    }

    #[test]
    fn a_rolling_package_exists_on_the_distro_and_its_derivatives() {
        let arch = PackageSource {
            releases: Releases::Rolling { id: "arch" },
            ..from("x", (0, 0), Unnumbered::Old)
        };
        assert!(OsRelease::parse("ID=arch\n").ships(&arch));
        assert!(OsRelease::parse("ID=manjaro\nID_LIKE=arch\n").ships(&arch));
        assert!(!OsRelease::parse(UBUNTU_24).ships(&arch));
    }
}
