//! The GPU memory budget: does an engine fit, and whom to evict if not.

use model_catalog::MiB;
use serde::{Deserialize, Serialize};

use crate::{EngineId, MonoMs, UnitSpec};

/// An engine the supervisor may run: its id, what it needs, how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpec {
    pub id: EngineId,
    /// `weights + kv(context) + overhead` (`VramEstimate::need`).
    pub need: MiB,
    pub unit: UnitSpec,
}

/// GPU memory as last observed. Others (a ComfyUI run, a game) are observed, never killed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuMemory {
    pub total: MiB,
    pub used_by_others: MiB,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum BudgetVerdict {
    Fits,
    /// Stop these first, least recently used first; then it fits.
    EvictFirst(Vec<EngineId>),
    NoRoom {
        need: MiB,
        free: MiB,
    },
}

/// Pure. `free = total - used_by_others - sum(running)`. If `need + headroom <= free` the verdict
/// is `Fits`; otherwise evict idle engines, least recently used first, until it fits; if that is
/// still not enough, `NoRoom`. `running` carries each engine's memory and its last use; an engine
/// used within the probe interval is in a turn and is never evicted.
pub fn budget(
    want: &EngineSpec,
    running: &[(EngineId, MiB, MonoMs)],
    gpu: GpuMemory,
    headroom: MiB,
) -> BudgetVerdict {
    let _ = (want, running, gpu, headroom);
    todo!("budget: fit, LRU eviction of idle engines, NoRoom with numbers")
}
