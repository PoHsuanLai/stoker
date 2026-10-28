//! Reading this computer for the engine manifest. Files only: nothing here runs a program it
//! finds, and every path and root is an argument (no environment, no guessing a home directory).
//!
//! - [`read_host`] and [`read_gpus`] make `engine-manifest`'s `Host` from the kernel's files and
//!   `os-release`.
//! - [`Roots`] and [`InstallDir`] say where installs live, for everyone or for one person, and
//!   [`Record`] is what an install leaves.
//! - [`Finder`] says which engine is already here and where it came from.
//! - [`Verifier`] is the check before a person's own install starts: it must still be the pinned
//!   tree. [`seal_of`] and [`entry_digests`] are the walks it makes.

mod found;
mod host;
mod layout;
mod seal;
mod verify;

pub use found::{Finder, Found, Place, pointed};
pub use host::{read_gpus, read_host};
pub use layout::{InstallDir, Record, RecordError, Roots, Scope};
pub use seal::{SealError, file_digest, seal_of};
pub use verify::{Verdict, Verifier, entry_digests};
