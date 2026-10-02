//! The head of a response: what the codec learns before the first byte of the body.
//!
//! Without it a codec cannot tell an error body from stream data, cannot read `Retry-After`,
//! and cannot refuse an HTML page that a proxy served with status 200. The shape follows rig's
//! transport (`Opened`), reduced to what we use: no free header map, only the fields below.

use serde::{Deserialize, Serialize};

use crate::HttpStatus;

/// A request id a server echoed back (`x-request-id`), kept for support and for the audit trail.
/// Opaque, 1 to 128 printable ASCII bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);

/// The text is not 1 to 128 printable ASCII bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a request id is 1 to 128 printable ASCII bytes")]
pub struct RequestIdError;

impl RequestId {
    pub fn new(text: impl Into<String>) -> Result<Self, RequestIdError> {
        let text = text.into();
        let ok = (1..=128).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic());
        if ok {
            Ok(Self(text))
        } else {
            Err(RequestIdError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RequestId {
    type Error = RequestIdError;
    fn try_from(text: String) -> Result<Self, RequestIdError> {
        RequestId::new(text)
    }
}

impl From<RequestId> for String {
    fn from(id: RequestId) -> String {
        id.0
    }
}

/// Seconds a server asked us to wait (`Retry-After`, the seconds form only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WaitSeconds(pub u32);

/// What the `Content-Type` says the body is. `Html` catches an error page served as 200.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    EventStream,
    Json,
    NdJson,
    Html,
    Other,
}

/// The status and the few headers a codec may read. Delivered once, before any chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseHead {
    pub status: HttpStatus,
    pub body: BodyKind,
    pub retry_after: Option<WaitSeconds>,
    pub request_id: Option<RequestId>,
}

/// A duration in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WaitMs(pub u32);
