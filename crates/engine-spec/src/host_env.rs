//! What an engine's unit gets besides its command line.

use std::path::PathBuf;

/// Environment variables for an engine's process, and folders its sandbox may read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostEnv {
    /// Variable names and values, in the order they are set.
    pub vars: Vec<(&'static str, String)>,
    /// Folders bound read-only into the engine's sandbox.
    pub reads: Vec<PathBuf>,
}
