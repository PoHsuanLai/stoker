//! The seams the daemon fills.

use crate::{EngineId, ExitCode, GpuMemory, Probe, UnitSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("the host refused to start the engine")]
    Refused,
    #[error("the engine is not running")]
    NotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GpuError {
    #[error("no GPU tool answered")]
    Unavailable,
    #[error("the GPU tool's output did not parse")]
    Unreadable,
}

/// Starts and stops engines: transient systemd units, child processes, or a fake.
pub trait EngineHost: Send + Sync {
    fn spawn(
        &self,
        id: &EngineId,
        unit: &UnitSpec,
    ) -> impl Future<Output = Result<(), HostError>> + Send;
    fn stop(&self, id: &EngineId) -> impl Future<Output = Result<(), HostError>> + Send;
    /// Completes when the engine's process ends.
    fn exited(&self, id: &EngineId) -> impl Future<Output = ExitCode> + Send;
}

/// `GET /health` over the engine's socket.
pub trait ReadyProbe: Send + Sync {
    fn probe(&self, id: &EngineId) -> impl Future<Output = Probe> + Send;
}

/// `nvidia-smi` csv output through an injected runner.
pub trait GpuProbe: Send + Sync {
    fn memory(&self) -> impl Future<Output = Result<GpuMemory, GpuError>> + Send;
}
