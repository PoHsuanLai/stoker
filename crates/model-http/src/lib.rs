//! HTTP to a model endpoint: a local engine on a Unix socket, a loopback port, or a
//! cloud host over TLS, possibly through the egress proxy.
//!
//! The decoder of server-sent events is pure; the client is the only part that does I/O.

mod auth;
mod client;
mod sse;
mod target;

pub use auth::{AuthHeader, HeaderName, Secret};
pub use client::{BodySink, ChunkFlow, HttpClient, HttpError, HttpStatus, JsonBody};
pub use sse::{EventName, SseDecoder, SseError, SseEvent};
pub use target::{HostName, HttpEndpoint, HttpTarget, Port, Proxy, UrlPath};
