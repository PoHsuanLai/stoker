//! The provider trait and its errors.

use serde::{Deserialize, Serialize};

use crate::{ModelName, RetrySeconds, ServerStatus, Tokens, TurnEnd, TurnRequest, TurnSink};

/// One endpoint: a running engine or a cloud account.
///
/// Cancellation is drop: the future is cancel-safe, and dropping it closes the connection, which
/// aborts generation in vLLM and llama-server. A sink answering `Flow::Stop` ends the turn early
/// with `StopReason::EndTurn`.
pub trait Provider: Send + Sync {
    /// The models this endpoint serves (`/v1/models`, `/props`).
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send;

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: ModelName,
    /// The context the engine actually loaded (llama.cpp `n_ctx`, vLLM `max_model_len`); the
    /// planner packs against this one.
    pub loaded_context: Tokens,
    /// The context the model was trained for.
    pub trained_context: Tokens,
}

/// Each arm is something a caller acts on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ProviderError {
    #[error("the endpoint is unreachable")]
    Unreachable,
    #[error("the engine is not ready")]
    NotReady,
    #[error("the turn timed out")]
    Timeout,
    #[error("rate limited, retry in {0:?}")]
    RateLimited(RetrySeconds),
    #[error("unauthorized")]
    Unauthorized,
    /// A 5xx the server answered with. Never carries the body (it can echo the prompt).
    #[error("the server failed with status {0:?}")]
    Server(ServerStatus),
    #[error("the prompt exceeds the context limit of {limit:?}")]
    ContextOverflow { limit: Tokens },
    #[error("the endpoint rejected the request: {0}")]
    BadRequest(String),
    #[error("the model refused: {0}")]
    Refused(String),
    #[error("the reply could not be read: {0}")]
    Unreadable(String),
}
