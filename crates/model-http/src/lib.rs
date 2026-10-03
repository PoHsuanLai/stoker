//! HTTP to a model endpoint: a local engine on a Unix socket, a loopback port, or a
//! cloud host over TLS, possibly through the egress proxy.
//!
//! The decoders of server-sent events and of NDJSON are pure; the transport is the only part
//! that does I/O, and it sits behind the `hyper` feature that only a daemon turns on, so the
//! crates that need only the seam types reach no HTTP stack and no runtime. A codec (`model-wire`) builds an `Exchange` and reads the response from a
//! `BodySink`, which sees the `ResponseHead` before the first byte of the body.

mod auth;
mod client;
mod exchange;
mod head;
#[cfg(feature = "hyper")]
mod hyper_client;
mod ndjson;
mod sse;
mod target;

pub use auth::{AuthHeader, ExtraHeader, HeaderName, Secret};
pub use client::{BodySink, ChunkFlow, HttpClient, HttpError, HttpStatus, JsonBody};
pub use exchange::{Exchange, Framing, RouteRoot, Timeouts, Transport, Verb};
pub use head::{BodyKind, RequestId, RequestIdError, ResponseHead, WaitMs, WaitSeconds};
pub use ndjson::{LineError, NdjsonDecoder};
pub use sse::{EventName, SseDecoder, SseError, SseEvent};
pub use target::{HostName, HttpEndpoint, HttpTarget, Port, Proxy, UrlPath};
