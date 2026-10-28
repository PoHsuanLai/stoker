//! Where the engines' programs and the weights cache are.

use std::collections::BTreeMap;
use std::path::PathBuf;

use model_catalog::EngineKind;
use serde::{Deserialize, Serialize};

/// An absolute path to an engine's program.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProgramPath(pub PathBuf);

/// Where the engines' programs and the weights cache are, from settings
/// (`ai.engine.<slug>.python` or `.path`); never from the environment. An engine with no program
/// listed cannot be started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnginePaths {
    pub programs: BTreeMap<EngineKind, ProgramPath>,
    pub hf_cache: PathBuf,
}

impl EnginePaths {
    /// Paths with a weights cache and no programs yet.
    pub fn new(hf_cache: PathBuf) -> Self {
        Self {
            programs: BTreeMap::new(),
            hf_cache,
        }
    }

    /// The same with `program` as the program of `kind`.
    #[must_use]
    pub fn with_program(mut self, kind: EngineKind, program: ProgramPath) -> Self {
        self.programs.insert(kind, program);
        self
    }

    /// The program that runs an engine of this kind, when settings list one.
    pub fn program(&self, kind: EngineKind) -> Option<&ProgramPath> {
        self.programs.get(&kind)
    }
}
