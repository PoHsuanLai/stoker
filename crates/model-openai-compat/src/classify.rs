//! Reading an error response: the status class first, the `{"error": ...}` envelope for the kind.

use model_http::{BodyKind, ResponseHead};
use model_provider::{ProviderError, RetrySeconds, ServerStatus, Tokens};
use serde_json::Value;

use crate::envelope::envelope_error;

/// Maps a non-success response, or an HTML page served as 200, to what a caller acts on. The
/// result never carries the body: only the error's type slug (already bounded by the envelope
/// reader) and numbers survive.
pub(crate) fn classify(head: &ResponseHead, body: &[u8]) -> ProviderError {
    let status = head.status.0;
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let envelope = parsed.as_ref().and_then(envelope_error);
    let wait = RetrySeconds(head.retry_after.map_or(0, |w| w.0));
    if (200..300).contains(&status) {
        return match (head.body, envelope) {
            (BodyKind::Html, _) => {
                ProviderError::Unreadable("an HTML page was served instead of a reply".into())
            }
            (_, Some(error)) => error,
            (_, None) => ProviderError::Unreadable("the reply is not an answer".into()),
        };
    }
    match status {
        401 | 403 => ProviderError::Unauthorized,
        408 => ProviderError::Timeout,
        429 => ProviderError::RateLimited(wait),
        500..=599 => match envelope {
            Some(error @ (ProviderError::NotReady | ProviderError::ContextOverflow { .. })) => {
                error
            }
            _ => ProviderError::Server(ServerStatus(status)),
        },
        _ => overflow_in_message(parsed.as_ref())
            .map(|limit| ProviderError::ContextOverflow { limit })
            .or(envelope)
            .map(|error| match error {
                ProviderError::RateLimited(RetrySeconds(0)) => ProviderError::RateLimited(wait),
                other => other,
            })
            .unwrap_or_else(|| ProviderError::BadRequest(format!("http_{status}"))),
    }
}

/// vLLM says `This model's maximum context length is 4096 tokens`: only the number is read, the
/// rest of the message can echo the prompt.
fn overflow_in_message(body: Option<&Value>) -> Option<Tokens> {
    const MARKER: &str = "maximum context length is ";
    let message = body?.get("error")?.get("message")?.as_str()?;
    let after = &message[message.find(MARKER)? + MARKER.len()..];
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().map(Tokens)
}
