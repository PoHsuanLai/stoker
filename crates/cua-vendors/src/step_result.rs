//! What happened to each call of the previous step.

use model_provider::ToolCallId;
use serde::{Deserialize, Serialize};

/// The outcome of one call the model made. Vendors that batch stop at the first failure, so the
/// calls after it are `NotRun`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum StepResult {
    Done(ToolCallId),
    Refused { id: ToolCallId, why: String },
    NotRun(ToolCallId),
}
