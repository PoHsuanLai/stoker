//! The client seam: one request, the response body pushed into a sink as it arrives.

use serde::{Deserialize, Serialize};

use crate::{HttpEndpoint, UrlPath};

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

/// A sink's answer to a chunk: keep reading, or close the connection (which aborts generation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkFlow {
    Continue,
    Stop,
}

/// Receives the response body in chunks.
pub trait BodySink: Send {
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
}

/// One endpoint's HTTP client. Dropping a request future closes the connection.
#[derive(Debug, Clone)]
pub struct HttpClient {
    endpoint: HttpEndpoint,
}

impl HttpClient {
    pub fn new(endpoint: HttpEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &HttpEndpoint {
        &self.endpoint
    }

    /// `GET path`, the body pushed into `sink`. A non-2xx answer is `HttpError::Status`.
    pub fn get<K: BodySink>(
        &self,
        path: &UrlPath,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let _ = (&self.endpoint, path, &mut *sink);
        async { todo!("HttpClient::get over hyper") }
    }

    /// `POST path` with a JSON body, the response body pushed into `sink`.
    pub fn post_json<K: BodySink>(
        &self,
        path: &UrlPath,
        body: &JsonBody,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let _ = (&self.endpoint, path, body, &mut *sink);
        async { todo!("HttpClient::post_json over hyper") }
    }
}
