//! Engine lifecycle for local model servers (llama-server, vLLM).
//!
//! `step` is a pure machine: time is an input and effects are returned as values. The daemon
//! carries the effects out through three seams: [`EngineHost`] starts and stops, [`ReadyProbe`]
//! asks whether an engine answers, [`GpuProbe`] reads free memory.
//!
//! ```
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! use engine_supervisor::{
//!     BudgetVerdict, EngineId, EngineSpec, GpuAccess, GpuMemory, MonoMs, Network, ProgramPath,
//!     Sandbox, UnitSpec, budget,
//! };
//! use model_catalog::MiB;
//!
//! let want = EngineSpec {
//!     id: EngineId("llama:small".into()),
//!     need: MiB(8_000),
//!     unit: UnitSpec {
//!         program: ProgramPath(PathBuf::from("/usr/bin/engine")),
//!         args: vec![],
//!         env: vec![],
//!         sandbox: Sandbox {
//!             network: Network::None,
//!             read: vec![],
//!             write: vec![],
//!             gpu: GpuAccess::Nvidia,
//!             memory_max: MiB(12_000),
//!         },
//!     },
//! };
//! let gpu = GpuMemory { total: MiB(16_000), used_by_others: MiB(1_000) };
//! // Nothing else of ours runs and 15 000 MiB are free: it fits.
//! let verdict = budget(&want, &[], gpu, MiB(500), MonoMs(0), Duration::from_secs(5));
//! assert_eq!(verdict, BudgetVerdict::Fits);
//! ```

mod budget;
mod host;
mod state;
mod step;
mod unit;

#[cfg(test)]
mod testutil;

#[cfg(feature = "testing")]
mod fakes;

pub use budget::{BudgetVerdict, EngineSpec, GpuMemory, budget};
#[cfg(feature = "testing")]
pub use fakes::{FakeEngineHost, FakeGpu, FakeReadyProbe, HostCall};
pub use host::{EngineHost, GpuError, GpuProbe, HostError, ReadyProbe};
pub use state::{Attempt, EngineFailure, EngineId, EngineState, ExitCode, MonoMs, Probe};
pub use step::{Supervisor, SupervisorConfig, SupervisorIn, SupervisorOut, step};
pub use unit::{
    EnginePaths, EnvPair, GpuAccess, Network, ProgramPath, Sandbox, SocketPath, UnitSpec, command,
};
