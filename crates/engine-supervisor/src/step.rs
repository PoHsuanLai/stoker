//! The lifecycle machine.

use std::time::Duration;

use model_catalog::MiB;
use serde::{Deserialize, Serialize};

use crate::{
    Attempt, BudgetVerdict, EngineId, EngineSpec, EngineState, ExitCode, GpuMemory, MonoMs, Probe,
    UnitSpec,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SupervisorIn {
    /// A caller wants this engine ready.
    Want(EngineId),
    /// A turn used this engine just now.
    Used(EngineId),
    Exited {
        id: EngineId,
        code: ExitCode,
    },
    Probed {
        id: EngineId,
        probe: Probe,
    },
    Gpu(GpuMemory),
    Tick(MonoMs),
}

/// What the daemon must do next. `step` never does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorOut {
    Spawn(EngineId, UnitSpec),
    Stop(EngineId),
    Probe(EngineId),
    Changed(EngineId, EngineState),
    /// Deliver a `Tick` at this time.
    WakeAt(MonoMs),
}

/// The values of the six `ai.engine.*` settings (design/22 rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisorConfig {
    pub idle_unload: Duration,
    pub start_timeout: Duration,
    pub probe_every: Duration,
    pub max_attempts: Attempt,
    pub backoff: (Duration, Duration),
    pub headroom: MiB,
}

impl Default for SupervisorConfig {
    /// 600 s idle unload, 180 s start timeout (vLLM cold start), probe every 500 ms, 3
    /// attempts, backoff 2 s to 30 s, 1024 MiB headroom.
    fn default() -> Self {
        SupervisorConfig {
            idle_unload: Duration::from_secs(600),
            start_timeout: Duration::from_secs(180),
            probe_every: Duration::from_millis(500),
            max_attempts: Attempt(3),
            backoff: (Duration::from_secs(2), Duration::from_secs(30)),
            headroom: MiB(1024),
        }
    }
}

/// Every engine the daemon may run, its state, and the GPU as last observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervisor {
    engines: Vec<(EngineSpec, EngineState)>,
    gpu: GpuMemory,
}

impl Supervisor {
    /// All engines `Stopped`.
    pub fn new(specs: Vec<EngineSpec>, gpu: GpuMemory) -> Supervisor {
        let engines = specs
            .into_iter()
            .map(|spec| (spec, EngineState::Stopped))
            .collect();
        Supervisor { engines, gpu }
    }

    pub fn state(&self, id: &EngineId) -> Option<&EngineState> {
        self.engines
            .iter()
            .find(|(spec, _)| &spec.id == id)
            .map(|(_, state)| state)
    }

    pub fn gpu(&self) -> GpuMemory {
        self.gpu
    }
}

/// The only transition: `(state, input) -> (state, effects)`. Time arrives as `Tick` and in
/// each input's context; the table is design: models §4.1.
pub fn step(
    s: Supervisor,
    input: SupervisorIn,
    cfg: &SupervisorConfig,
) -> (Supervisor, Vec<SupervisorOut>) {
    let _ = (s, input, cfg, BudgetVerdict::Fits);
    todo!("step: the engine lifecycle table")
}
