//! Scripted fakes of the three seams.

use std::sync::Mutex;

use crate::{
    EngineHost, EngineId, ExitCode, GpuError, GpuMemory, GpuProbe, HostError, Probe, ReadyProbe,
    UnitSpec,
};

/// A call a [`FakeEngineHost`] received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCall {
    Spawn(EngineId, UnitSpec),
    Stop(EngineId),
}

/// Records spawns and stops; `exited` answers at once with the scripted code.
#[derive(Debug)]
pub struct FakeEngineHost {
    exit: ExitCode,
    calls: Mutex<Vec<HostCall>>,
}

impl FakeEngineHost {
    pub fn new(exit: ExitCode) -> Self {
        Self {
            exit,
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<HostCall> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }

    fn record(&self, call: HostCall) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
    }
}

impl EngineHost for FakeEngineHost {
    fn spawn(
        &self,
        id: &EngineId,
        unit: &UnitSpec,
    ) -> impl Future<Output = Result<(), HostError>> + Send {
        self.record(HostCall::Spawn(id.clone(), unit.clone()));
        std::future::ready(Ok(()))
    }

    fn stop(&self, id: &EngineId) -> impl Future<Output = Result<(), HostError>> + Send {
        self.record(HostCall::Stop(id.clone()));
        std::future::ready(Ok(()))
    }

    fn exited(&self, _id: &EngineId) -> impl Future<Output = ExitCode> + Send {
        std::future::ready(self.exit)
    }
}

/// Always answers the same probe.
#[derive(Debug, Clone, Copy)]
pub struct FakeReadyProbe(pub Probe);

impl ReadyProbe for FakeReadyProbe {
    fn probe(&self, _id: &EngineId) -> impl Future<Output = Probe> + Send {
        std::future::ready(self.0)
    }
}

/// Always reports the same memory.
#[derive(Debug, Clone, Copy)]
pub struct FakeGpu(pub GpuMemory);

impl GpuProbe for FakeGpu {
    fn memory(&self) -> impl Future<Output = Result<GpuMemory, GpuError>> + Send {
        std::future::ready(Ok(self.0))
    }
}
