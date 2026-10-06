//! The client: one endpoint, and the types a transport and a sink share. The request seam is
//! `Transport` (`exchange`); the response body is pushed into a sink as it arrives.

use serde::{Deserialize, Serialize};

use crate::{HttpEndpoint, ResponseHead};

/// An HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HttpStatus(pub u16);

/// A request body that is JSON text.
#[derive(Clone, PartialEq, Eq)]
pub struct JsonBody(pub String);

// A body can hold what the person asked: Debug shows the length only.
impl core::fmt::Debug for JsonBody {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "JsonBody(<{} bytes>)", self.0.len())
    }
}

/// The `Content-Type` of a request body: `application/json`, or a `multipart/form-data` value with
/// its boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentType(pub String);

/// A request body that is bytes, with the type that says what they are. What is in it can be
/// audio the person spoke: `Debug` shows the length only.
#[derive(Clone, PartialEq, Eq)]
pub struct RawBody {
    pub content_type: ContentType,
    pub bytes: Vec<u8>,
}

impl core::fmt::Debug for RawBody {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RawBody(<{} bytes>)", self.bytes.len())
    }
}

/// A sink's answer to a chunk: keep reading, or close the connection (which aborts generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkFlow {
    Continue,
    Stop,
}

/// Receives a response: its head once, then the body in chunks (already split into frames when
/// the exchange asked for `Framing::Sse` or `Ndjson`).
pub trait BodySink: Send {
    /// Once, before any chunk. `Stop` closes the connection without reading the body.
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow;
    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum HttpError {
    #[error("could not connect")]
    Connect,
    #[error("the connection timed out")]
    Timeout,
    #[error("the TLS handshake failed")]
    Tls,
    #[error("the connection broke mid-response")]
    Broken,
    #[error("the server answered {0:?}")]
    Status(HttpStatus),
    /// The server answered non-success. The head and the body already went to the sink, so the
    /// codec classifies them; the error carries nothing (a body can echo the prompt).
    #[error("the server rejected the request")]
    Rejected,
    /// A replay transport had no recorded exchange for the request (`ReplayTransport::misses`
    /// says which and why). Only a test transport answers it; a real one never does.
    #[error("no recorded exchange answers this request")]
    ReplayMiss,
}

/// One DER-encoded X.509 certificate.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DerCertificate(pub Vec<u8>);

impl core::fmt::Debug for DerCertificate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "DerCertificate(<{} bytes>)", self.0.len())
    }
}

/// Which certificates may vouch for a `Tls` target's server. Verification of the chain, the
/// validity dates and the host name is always on; this only says which roots it ends in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TlsRoots {
    /// The platform's trust store (`rustls-native-certs`).
    #[default]
    Platform,
    /// Only these roots, and nothing else: the seam for a test's scratch CA.
    Only(Vec<DerCertificate>),
}

/// One endpoint's HTTP client; it is a [`crate::Transport`].
#[derive(Debug, Clone)]
pub struct HttpClient {
    endpoint: HttpEndpoint,
    roots: TlsRoots,
}

impl HttpClient {
    /// A client trusting the platform's roots for a `Tls` target.
    pub fn new(endpoint: HttpEndpoint) -> Self {
        Self::with_roots(endpoint, TlsRoots::Platform)
    }

    pub fn with_roots(endpoint: HttpEndpoint, roots: TlsRoots) -> Self {
        Self { endpoint, roots }
    }

    pub fn roots(&self) -> &TlsRoots {
        &self.roots
    }

    pub fn endpoint(&self) -> &HttpEndpoint {
        &self.endpoint
    }
}
