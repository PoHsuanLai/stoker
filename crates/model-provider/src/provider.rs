//! The provider trait and its errors.

use serde::{Deserialize, Serialize};

use crate::{
    ModelName, ProviderDetail, RetrySeconds, ServerStatus, Tokens, TurnEnd, TurnRequest, TurnSink,
};

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
    /// planner packs against this one. `Tokens(0)` when the server does not say (a LiteLLM
    /// model list): the caller then uses the catalog entry's context.
    pub loaded_context: Tokens,
    /// The context the model was trained for; the loaded one when the server reports only one
    /// of the two, `Tokens(0)` when it reports neither.
    pub trained_context: Tokens,
}

/// What separates a `BadRequest`'s slug from the provider's message.
const MESSAGE_SEP: &str = ": ";

fn slug_of(text: &str) -> &str {
    text.split_once(MESSAGE_SEP).map_or(text, |(slug, _)| slug)
}

impl ProviderError {
    /// A rejected request that carries the provider's (already redacted) message, kept out of
    /// `Display`.
    pub fn bad_request_with_message(slug: &str, message: &str) -> Self {
        if message.is_empty() {
            ProviderError::BadRequest(slug.to_owned())
        } else {
            ProviderError::BadRequest(format!("{slug}{MESSAGE_SEP}{message}"))
        }
    }

    /// The provider's own words behind a refusal, for a log or a debug view: never for a person
    /// or a model, because they can quote the prompt. `None` when the provider gave none.
    pub fn provider_message(&self) -> Option<&str> {
        let message = match self {
            ProviderError::AuthRejected(detail) | ProviderError::PaymentRequired(detail) => {
                detail.message.as_str()
            }
            ProviderError::BadRequest(text) => text.split_once(MESSAGE_SEP)?.1,
            _ => return None,
        };
        (!message.is_empty()).then_some(message)
    }
}

/// Each arm is something a caller acts on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
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
    /// The provider knows the credentials and refused them (403), with its reason when it gave
    /// one. A missing or wrong key (401) is `Unauthorized`: signing in again can fix that, and
    /// this it cannot.
    #[error("the endpoint refused the credentials ({0})")]
    AuthRejected(ProviderDetail),
    /// The account is out of credit (402). Retrying cannot help; the person has to top up or pick
    /// another model.
    #[error("the account is out of credit ({0})")]
    PaymentRequired(ProviderDetail),
    /// A 5xx the server answered with. Never carries the body (it can echo the prompt).
    #[error("the server failed with status {0:?}")]
    Server(ServerStatus),
    #[error("the prompt exceeds the context limit of {limit:?}")]
    ContextOverflow { limit: Tokens },
    /// The text is the error's type slug (`http_400 invalid_request_error`), optionally followed by
    /// `": "` and the provider's own message. `Display` prints the part before the separator only,
    /// because the message can quote the prompt; [`ProviderError::provider_message`] has the rest.
    #[error("the endpoint rejected the request: {}", slug_of(.0))]
    BadRequest(String),
    #[error("the model refused: {0}")]
    Refused(String),
    #[error("the reply could not be read: {0}")]
    Unreadable(String),
}
