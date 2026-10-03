//! The error envelope a server writes, in a 200 stream or as an error body.

use model_provider::{ProviderError, RetrySeconds, ServerStatus, Tokens};
use serde_json::Value;

/// The error in `value` (`{"error": {...}}` or `{"error": "text"}`), mapped to what a caller
/// acts on. Only the kind of failure survives: the message can echo the prompt, so it is never
/// carried (`BadRequest` holds the error's type slug, at most 40 characters of `[a-z0-9_.-]`).
pub(crate) fn envelope_error(value: &Value) -> Option<ProviderError> {
    let error = value.get("error").filter(|e| !e.is_null())?;
    let field = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase()
    };
    let (kind, code) = (field("type"), field("code"));
    let has = |needle: &str| kind.contains(needle) || code.contains(needle);
    let numeric = error
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|c| u16::try_from(c).ok());
    Some(match () {
        _ if has("context_length") || has("context_window") || has("exceed_context") => {
            let limit = error.get("n_ctx").and_then(Value::as_u64).unwrap_or(0);
            ProviderError::ContextOverflow {
                limit: Tokens(u32::try_from(limit).unwrap_or(u32::MAX)),
            }
        }
        _ if has("rate_limit") || numeric == Some(429) => {
            ProviderError::RateLimited(RetrySeconds(0))
        }
        _ if has("auth") || has("api_key") || matches!(numeric, Some(401 | 403)) => {
            ProviderError::Unauthorized
        }
        _ if has("unavailable") || has("loading") => ProviderError::NotReady,
        _ if has("server") || has("overload") || matches!(numeric, Some(500..=599)) => {
            ProviderError::Server(ServerStatus(numeric.filter(|c| *c >= 500).unwrap_or(500)))
        }
        _ => ProviderError::BadRequest(slug(&kind)),
    })
}

fn slug(kind: &str) -> String {
    let clean: String = kind
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        .take(40)
        .collect();
    if clean.is_empty() {
        "error".to_owned()
    } else {
        clean
    }
}
