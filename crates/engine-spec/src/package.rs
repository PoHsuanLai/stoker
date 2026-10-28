//! A distro package that holds an engine, and the releases it was checked on. A package is only
//! ever named for a release that was checked to ship it.

/// The program that installs packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    /// Debian and Ubuntu.
    Apt,
    /// Fedora.
    Dnf,
    /// Arch.
    Pacman,
}

impl PackageManager {
    /// The program, by its fixed absolute path: nothing a caller or the environment names.
    pub fn program(self) -> &'static str {
        match self {
            Self::Apt => "/usr/bin/apt-get",
            Self::Dnf => "/usr/bin/dnf",
            Self::Pacman => "/usr/bin/pacman",
        }
    }

    /// The arguments before the package name: install without asking.
    pub fn install_args(self) -> &'static [&'static str] {
        match self {
            Self::Apt => &["install", "-y"],
            Self::Dnf => &["install", "-y"],
            Self::Pacman => &["-S", "--noconfirm", "--needed"],
        }
    }
}

/// Which releases a package is known to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Releases {
    /// Releases whose `ID` is this one, from this version on.
    From {
        /// `ID`.
        id: &'static str,
        /// The first release, (major, minor).
        version: (u32, u32),
        /// Whether a release with no version number (Debian testing) counts as new enough.
        unnumbered: Unnumbered,
    },
    /// Any release whose `ID` or `ID_LIKE` is this one (a rolling distro and its derivatives).
    Rolling {
        /// `ID` or one word of `ID_LIKE`.
        id: &'static str,
    },
}

/// How a release with no `VERSION_ID` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unnumbered {
    /// As new: Debian testing is ahead of every stable release.
    New,
    /// As old: no package can be promised.
    Old,
}

/// One distro package that holds an engine, and the releases it was checked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageSource {
    /// The manager that installs it.
    pub manager: PackageManager,
    /// The package's name.
    pub name: &'static str,
    /// Where it exists.
    pub releases: Releases,
    /// Whether the package's file list was checked to hold the engine's program at its usual
    /// path. A row that is not is installed all the same and the program looked for after.
    pub verified: Verified,
}

/// Whether a manifest row was checked against what it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verified {
    /// Checked on 2026-10-17 against the distro's own package listing.
    Yes,
    /// Not checked; see the row's comment.
    No,
}
