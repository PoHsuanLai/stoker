//! Wire cassettes: HTTP exchanges recorded and replayed at the `Transport` seam.
//!
//! The typed turn cassettes (`cassette`) are the test input for planner and agent tests: fast,
//! no codec in the loop. These catch codec regressions against real engines: the request a codec
//! built and the response an engine gave, as frames, so a diff reads. Replay is in process, at the
//! trait, with no socket and no loopback server.
//!
//! File: JSON Lines, `<name>.wire.jsonl`, a header line (the same [`CassetteHeader`] as the turn
//! files) then one [`WireExchange`] per line. The suffix keeps tools from mixing the two.
//!
//! Scrub policy, as data, applied at record time by the dev script and checked again by a test
//! over every committed file: every response header is dropped except the allowlist that
//! [`HeadPrint`] is; absolute model paths and `$HOME` become `/REDACTED_PATH` (llama.cpp echoes
//! the full GGUF path in `model`); a file carrying `Authorization`, a `sk-` style key or an email
//! is refused; nothing `Untrusted`-labelled or `Private` is ever recorded, and nothing is
//! recorded from a real user session.

use model_http::{BodyKind, EventName, HttpStatus, RouteRoot, UrlPath, Verb, WaitSeconds};
use model_provider::{JsonText, Seed};
use serde::{Deserialize, Serialize};

use crate::cassette::{read_jsonl, write_jsonl};
use crate::{CassetteError, CassetteHeader, InteractionId, SinkError};

/// The header of a wire cassette: the turn header, so one type carries version and engine stamp.
pub type WireHeader = CassetteHeader;

/// The request a codec built. The body is canonical JSON with image base64 replaced by an image
/// print and the model path scrubbed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRequest {
    pub verb: Verb,
    pub root: RouteRoot,
    pub path: UrlPath,
    pub body: Option<JsonText>,
}

/// The only response fields a cassette keeps: status, body kind and `Retry-After`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadPrint {
    pub status: HttpStatus,
    pub body: BodyKind,
    pub retry_after: Option<WaitSeconds>,
}

/// One frame: an SSE event name and data, or one NDJSON line (no event name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFrame {
    pub event: Option<EventName>,
    pub data: String,
}

/// The body, as frames (so a diff is readable), not bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum WireBody {
    Whole(String),
    Frames(Vec<WireFrame>),
}

/// How the exchange ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireEnd {
    Complete,
    /// The stream stopped without a terminal frame.
    Cut,
    /// The connection failed after frames.
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReply {
    pub head: HeadPrint,
    pub body: WireBody,
    pub end: WireEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireExchange {
    pub request: WireRequest,
    pub reply: WireReply,
}

/// A wire cassette in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireCassette {
    pub header: WireHeader,
    pub exchanges: Vec<WireExchange>,
}

impl WireCassette {
    /// The file's text: the header, then one line per exchange.
    pub fn to_jsonl(&self) -> String {
        write_jsonl(&self.header, &self.exchanges)
    }

    pub fn from_jsonl(text: &str) -> Result<WireCassette, CassetteError> {
        let (header, exchanges) = read_jsonl(text)?;
        Ok(WireCassette { header, exchanges })
    }
}

/// How a replay slices frames back into bytes. Tests run `Every(ByteStep(1))` and `Seeded` so a
/// decoder that depends on chunking fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ChunkPlan {
    /// The whole body in one chunk.
    Whole,
    /// One chunk per frame, with its line endings.
    Lines,
    /// Chunks of this many bytes.
    Every(ByteStep),
    /// Chunk sizes drawn from a seeded generator.
    Seeded(Seed),
}

/// A chunk size in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteStep(pub u32);

/// Where recorded exchanges go: a file in a dev script, a `Vec` in a test.
pub trait WireSink: Send + Sync {
    fn write(&self, exchange: &WireExchange) -> Result<(), SinkError>;
}

/// A request the replay could not serve, kept for the test to read (a transport can only answer
/// `HttpError::ReplayMiss`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireMiss {
    #[error("the wire cassette has no exchange left")]
    Exhausted,
    #[error("exchange {index:?} was recorded for a different request")]
    Mismatch {
        index: InteractionId,
        want: Box<WireRequest>,
        got: Box<WireRequest>,
    },
}
