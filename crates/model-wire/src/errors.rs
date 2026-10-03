//! What a transport or a codec failure means to a caller: each becomes a `ProviderError` that
//! carries no body and no model text (a body can echo the prompt).

use model_http::{HttpError, ResponseHead};
use model_provider::{ProviderError, RetrySeconds, ServerStatus};

use crate::CodecError;

/// A connection that never opened or broke is `Unreachable`; a timeout is `Timeout`; an answer
/// the transport reports as a bare status is a `Server` error when it is 5xx and a bad request
/// otherwise. A replay with no exchange left for the request is a bad request: the code under test
/// asked for something its cassette does not hold.
pub fn http_error(error: HttpError) -> ProviderError {
    match error {
        HttpError::Connect | HttpError::Tls | HttpError::Broken => ProviderError::Unreachable,
        HttpError::Timeout => ProviderError::Timeout,
        HttpError::Status(status) if (500..600).contains(&status.0) => {
            ProviderError::Server(ServerStatus(status.0))
        }
        HttpError::Status(status) => ProviderError::BadRequest(format!("http_{}", status.0)),
        HttpError::Rejected => ProviderError::Unreadable("rejected without a reply".into()),
        HttpError::ReplayMiss => ProviderError::BadRequest("no recorded exchange".into()),
    }
}

/// A codec failure, in words that name the kind and never a value.
pub(crate) fn codec_error(error: CodecError) -> ProviderError {
    match error {
        CodecError::UnsupportedShape => {
            ProviderError::BadRequest("unsupported shape for this wire".into())
        }
        CodecError::NativeToolUnsupported => {
            ProviderError::BadRequest("native tools are not supported by this wire".into())
        }
        CodecError::Unreadable => ProviderError::Unreadable("a frame is not a chunk".into()),
        CodecError::Truncated => {
            ProviderError::Unreadable("the stream ended before a finish reason".into())
        }
        CodecError::BadToolArguments => {
            ProviderError::Unreadable("tool-call arguments are not valid JSON".into())
        }
    }
}

/// A rate limit that carries no seconds (an envelope has none) takes the head's `Retry-After`.
pub(crate) fn with_retry_after(error: ProviderError, head: Option<&ResponseHead>) -> ProviderError {
    match (error, head.and_then(|h| h.retry_after)) {
        (ProviderError::RateLimited(RetrySeconds(0)), Some(wait)) => {
            ProviderError::RateLimited(RetrySeconds(wait.0))
        }
        (error, _) => error,
    }
}
