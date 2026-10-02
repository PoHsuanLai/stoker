//! Engine lifecycle for local model servers (llama-server, vLLM).
//!
//! `step` is a pure machine: time is an input and effects are returned as values. The daemon
//! carries the effects out through three seams: [`EngineHost`] starts and stops, [`ReadyProbe`]
//! asks whether an engine answers, [`GpuProbe`] reads free memory.

mod budget;
mod host;
mod state;
mod step;
mod unit;

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
