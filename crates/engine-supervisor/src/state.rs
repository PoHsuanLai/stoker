//! The state of one engine, and the small units around it.

use model_catalog::MiB;
use serde::{Deserialize, Serialize};

/// `<engine kind>:<catalog id>`, for example `vllm:holo-3.1-4b`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineId(pub String);

/// Milliseconds on a monotonic clock the daemon owns. Time is an input, never read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonoMs(pub u64);

/// Which start attempt this is, from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Attempt(pub u8);

/// A process's exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExitCode(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineState {
    Stopped,
    Starting { since: MonoMs, attempt: Attempt },
    Ready { since: MonoMs, last_used: MonoMs },
    Stopping { since: MonoMs },
    Backoff { until: MonoMs, attempt: Attempt },
    Failed(EngineFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineFailure {
    NoRoom { need: MiB, free: MiB },
    Exited { code: ExitCode },
    NeverReady,
    BadProfile,
}

/// The answer of a readiness probe (`GET /health` over the engine's socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Probe {
    Ready,
    Loading,
    Down,
}
