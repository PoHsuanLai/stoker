//! Shared builders for the unit tests.

use std::path::PathBuf;

use model_catalog::MiB;

use crate::{EngineId, EngineSpec, GpuAccess, Network, ProgramPath, Sandbox, UnitSpec};

pub fn id(name: &str) -> EngineId {
    EngineId(name.to_owned())
}

pub fn spec(name: &str, need: u32) -> EngineSpec {
    EngineSpec {
        id: id(name),
        need: MiB(need),
        unit: UnitSpec {
            program: ProgramPath(PathBuf::from("/usr/bin/engine")),
            args: Vec::new(),
            env: Vec::new(),
            sandbox: Sandbox {
                network: Network::None,
                read: Vec::new(),
                write: Vec::new(),
                gpu: GpuAccess::Nvidia,
                memory_max: MiB(need),
            },
        },
    }
}
