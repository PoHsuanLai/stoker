//! Gets an engine onto this computer from `engine-manifest`'s pinned manifest.
//!
//! - [`Fetcher`] downloads one pinned file: over `https` only, resumable, cancellable, counted
//!   in a [`Meter`], and kept only if its size and SHA-256 are the pinned ones. The address it
//!   follows is a [`PinnedUrl`].
//! - [`Installer`] installs a manifest row for everyone or for one person: space checked, files
//!   fetched, unpacked in process with nothing let out of the install (or a Python environment
//!   filled from the row's hashed lock), the tree's seal compared with the pinned one, then
//!   renamed into place whole.
//! - [`install_package`] installs the distro's package instead.
//! - [`EngineInstalls`] is what a program that offers installs holds: what could be installed for
//!   an engine here ([`InstallChoice`]), the jobs it starts (for the person in this process, for
//!   everyone through `pkexec` and the helper, [`install_for_everyone`]) and where each stands.
//! - [`helper`] is the privileged side: the only arguments it accepts (an engine and a backend
//!   by name, never an address, path or version), the report lines it prints and the work it
//!   does as root. The `engine-helper` program is a thin shell around it.
//!
//! Nothing here chooses what to install or asks anyone: that is the caller's. The caller also
//! says where the helper is: a [`HelperProgram`] names `pkexec` and the helper by path.

mod elevated;
mod extract;
mod fetch;
pub mod helper;
mod installer;
mod jobs;
mod package;
mod pinned_url;
mod progress;
mod pyenv;
mod space;

pub use elevated::{HelperProgram, install_for_everyone};
pub use extract::{ExtractError, settle_modes, unpack};
pub use fetch::{FetchError, Fetcher};
pub use installer::{InstallError, Installer};
pub use jobs::{EngineInstalls, InstallChoice, Parts, Standing, StartError};
pub use package::install_package;
pub use pinned_url::{NotPinned, PinnedUrl};
pub use progress::{CancelToken, Meter};
pub use space::{NoSpace, free_bytes, require_space};
