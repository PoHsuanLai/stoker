//! HTTP to a model endpoint: a local engine on a Unix socket, a loopback port, or a
//! cloud host over TLS, possibly through the egress proxy.
//!
//! The decoders of server-sent events and of NDJSON are pure; the transport is the only part
//! that does I/O, and it sits behind the `hyper` feature that only a daemon turns on, so the
//! crates that need only the seam types reach no HTTP stack and no runtime. A codec (`model-wire`) builds an `Exchange` and reads the response from a
//! `BodySink`, which sees the `ResponseHead` before the first byte of the body.
//!
//! ```
//! use model_http::{
//!     AuthHeader, HostName, HttpClient, HttpEndpoint, HttpTarget, Port, Proxy, Timeouts, UrlPath,
//!     WaitMs,
//! };
//!
//! // A local engine on a loopback port, no credentials.
//! let endpoint = HttpEndpoint {
//!     target: HttpTarget::Tcp { host: HostName("127.0.0.1".into()), port: Port(8000) },
//!     proxy: Proxy::Direct,
//!     base: UrlPath("/v1".into()),
//!     auth: AuthHeader::None,
//!     headers: vec![],
//!     timeouts: Timeouts { connect: WaitMs(2_000), first_byte: WaitMs(60_000), idle: WaitMs(30_000) },
//! };
//! let client = HttpClient::new(endpoint.clone());
//! assert_eq!(client.endpoint(), &endpoint);
//! ```

mod auth;
mod client;
mod exchange;
mod head;
#[cfg(feature = "hyper")]
mod hyper_client;
mod ndjson;
mod sse;
mod target;
#[cfg(feature = "tls")]
mod tls;

pub use auth::{AuthHeader, ExtraHeader, HeaderName, Secret};
pub use client::{
    BodySink, ChunkFlow, ContentType, DerCertificate, HttpClient, HttpError, HttpStatus, JsonBody,
    RawBody, TlsRoots,
};
pub use exchange::{
    Exchange, Framing, RouteRoot, Timeouts, Transport, Upload, UploadTransport, Verb,
};
pub use head::{BodyKind, RequestId, RequestIdError, ResponseHead, WaitMs, WaitSeconds};
pub use ndjson::{LineError, NdjsonDecoder};
pub use sse::{EventName, SseDecoder, SseError, SseEvent};
pub use target::{HostName, HttpEndpoint, HttpTarget, Port, Proxy, UrlPath};
