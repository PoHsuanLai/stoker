//! Reading an error response: the status class first, the `{"error": ...}` envelope for the kind.

use model_http::{BodyKind, ResponseHead};
use model_provider::{ProviderDetail, ProviderError, RetrySeconds, ServerStatus, Tokens};
use serde_json::Value;

use crate::envelope::envelope_error;

/// Maps a non-success response, or an HTML page served as 200, to what a caller acts on. A
/// client error keeps the provider's status and a bounded, redacted excerpt of its message
/// (`ProviderDetail`), so the person and the logs see why: no credit, an unsupported parameter, a
/// refused key. A 5xx never carries the body, and neither does a context overflow.
pub(crate) fn classify(head: &ResponseHead, body: &[u8]) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let status = head.status.0;
    let bare = read(head, parsed.as_ref());
    if !(400..500).contains(&status) {
        return bare;
    }
    let detail = ProviderDetail::new(status, provider_message(parsed.as_ref()).unwrap_or(""));
    match bare {
        ProviderError::PaymentRequired(_) => ProviderError::PaymentRequired(detail),
        ProviderError::Unauthorized if !detail.message.is_empty() => {
            ProviderError::AuthRejected(detail)
        }
        ProviderError::BadRequest(slug) if !detail.message.is_empty() => {
            let prefix = if slug.starts_with("http_") {
                slug
            } else {
                format!("http_{status} {slug}")
            };
            ProviderError::BadRequest(format!("{prefix}: {}", detail.message))
        }
        other => other,
    }
}

/// `classify` without the provider's message: the speech endpoints use it, because their error
/// bodies can echo audio or text.
pub(crate) fn classify_bare(head: &ResponseHead, body: &[u8]) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    read(head, parsed.as_ref())
}

/// The message a provider put in an error body: `error.message`, `error` as text, `message` or
/// `detail`.
fn provider_message(body: Option<&Value>) -> Option<&str> {
    let value = body?;
    let error = value.get("error");
    error
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .or_else(|| error.and_then(Value::as_str))
        .or_else(|| value.get("message").and_then(Value::as_str))
        .or_else(|| value.get("detail").and_then(Value::as_str))
}

fn read(head: &ResponseHead, parsed: Option<&Value>) -> ProviderError {
    let status = head.status.0;
    let envelope = parsed.and_then(envelope_error);
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
        402 => ProviderError::PaymentRequired(ProviderDetail::new(status, "")),
        408 => ProviderError::Timeout,
        429 => ProviderError::RateLimited(wait),
        500..=599 => match envelope {
            Some(error @ (ProviderError::NotReady | ProviderError::ContextOverflow { .. })) => {
                error
            }
            _ => ProviderError::Server(ServerStatus(status)),
        },
        _ => overflow_in_message(parsed)
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
