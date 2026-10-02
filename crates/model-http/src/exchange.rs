//! The transport seam: one exchange (a request and its response) with the head delivered first.
//!
//! The wire (a codec in `model-wire`) builds an [`Exchange`] and reads the response; a
//! transport delivers it: hyper over TCP, a Unix socket or TLS, or later an in-process engine.
//! Wire fixtures are recorded and replayed at this seam (`model-replay::wire`).

use serde::{Deserialize, Serialize};

use crate::{BodySink, HttpClient, HttpError, HttpStatus, JsonBody, UrlPath, WaitMs};

/// How the transport splits the response body into frames before the codec sees it. The wire
/// chooses; the transport applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    /// Server-sent events: one frame per event's data.
    Sse,
    /// Newline-delimited JSON: one frame per line.
    Ndjson,
    /// The whole body is one frame.
    Whole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    Get,
    PostJson,
}

/// Where a path starts: llama-server keeps `/props`, `/health` and `/tokenize` at the server
/// root, outside the endpoint's `/v1` base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteRoot {
    /// Under `HttpEndpoint::base`.
    Base,
    /// At the server root.
    Server,
}

/// How long a transport waits at each stage before it gives up with `HttpError::Timeout`. A hung
/// engine must not freeze a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeouts {
    pub connect: WaitMs,
    pub first_byte: WaitMs,
    /// Between two chunks of the body.
    pub idle: WaitMs,
}

/// One request: what a codec asks a transport to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    pub verb: Verb,
    pub root: RouteRoot,
    pub path: UrlPath,
    pub body: Option<JsonBody>,
    pub framing: Framing,
}

/// Delivers an [`Exchange`]. Dropping the future closes the connection, which aborts generation.
pub trait Transport: Send + Sync {
    /// Sends `ex`; delivers `head`, then the body in chunks, to `sink`. `Err` only for connect,
    /// timeout, TLS, a broken connection, or `Rejected` (the server answered non-success; the
    /// head and body already went to the sink).
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send;
}

impl Transport for HttpClient {
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let _ = (self.endpoint(), ex, &mut *sink);
        async {
            todo!(
                "Transport for HttpClient over hyper: Tcp, Unix or Tls by endpoint.target, proxy via endpoint.proxy"
            )
        }
    }
}
