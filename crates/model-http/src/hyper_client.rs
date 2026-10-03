//! `Transport for HttpClient` over hyper: HTTP/1.1 on a TCP or Unix socket, one connection per
//! exchange (dropping the future drops the connection, which aborts generation in the engine).
//!
//! Not yet built: TLS and the egress proxy, which arrive with the first cloud backend (the local
//! engines are plain sockets). A `Tls` target answers `HttpError::Tls` and a `Via` proxy
//! `HttpError::Connect` without sending anything.

use std::future::Future;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HOST, HeaderName as WireName, HeaderValue, RETRY_AFTER,
};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpStream, UnixStream};
use tokio::time::timeout;

use crate::{
    AuthHeader, BodyKind, BodySink, ChunkFlow, Exchange, Framing, HttpClient, HttpEndpoint,
    HttpError, HttpStatus, HttpTarget, Proxy, RequestId, ResponseHead, RouteRoot, Timeouts,
    Transport, Upload, UploadTransport, Verb, WaitMs, WaitSeconds,
};

impl Transport for HttpClient {
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let endpoint = self.endpoint();
        let parts = Parts {
            method: match ex.verb {
                Verb::Get => Method::GET,
                Verb::PostJson => Method::POST,
            },
            root: ex.root,
            path: ex.path.0.as_str(),
            framing: ex.framing,
            content_type: ex.body.as_ref().map(|_| "application/json"),
            body: ex
                .body
                .as_ref()
                .map_or_else(Bytes::new, |b| Bytes::from(b.0.clone())),
        };
        send(endpoint, parts, sink)
    }
}

impl UploadTransport for HttpClient {
    fn upload<K: BodySink>(
        &self,
        up: &Upload,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let parts = Parts {
            method: Method::POST,
            root: up.root,
            path: up.path.0.as_str(),
            framing: up.framing,
            content_type: Some(up.body.content_type.0.as_str()),
            body: Bytes::from(up.body.bytes.clone()),
        };
        send(self.endpoint(), parts, sink)
    }
}

/// What a request is, whichever seam asked for it.
struct Parts<'a> {
    method: Method,
    root: RouteRoot,
    path: &'a str,
    framing: Framing,
    content_type: Option<&'a str>,
    body: Bytes,
}

/// Connects to the endpoint and runs one request on the connection.
async fn send<K: BodySink>(
    endpoint: &HttpEndpoint,
    parts: Parts<'_>,
    sink: &mut K,
) -> Result<HttpStatus, HttpError> {
    let request = build(endpoint, parts)?;
    let timeouts = &endpoint.timeouts;
    match (&endpoint.proxy, &endpoint.target) {
        (Proxy::Via(_), _) => Err(HttpError::Connect),
        (Proxy::Direct, HttpTarget::Tls { .. }) => Err(HttpError::Tls),
        (Proxy::Direct, HttpTarget::Tcp { host, port }) => {
            let stream = within(
                timeouts.connect,
                TcpStream::connect((host.0.as_str(), port.0)),
            )
            .await?
            .map_err(|_| HttpError::Connect)?;
            talk(stream, request, timeouts, sink).await
        }
        (Proxy::Direct, HttpTarget::Unix(path)) => {
            let stream = within(timeouts.connect, UnixStream::connect(path))
                .await?
                .map_err(|_| HttpError::Connect)?;
            talk(stream, request, timeouts, sink).await
        }
    }
}

/// Runs `future` for at most `wait`; an overrun is `HttpError::Timeout`.
async fn within<T>(wait: WaitMs, future: impl Future<Output = T>) -> Result<T, HttpError> {
    timeout(Duration::from_millis(u64::from(wait.0)), future)
        .await
        .map_err(|_| HttpError::Timeout)
}

/// The request line, headers and body of `parts` against `endpoint`. A header value that cannot
/// be written (a newline in a secret) is a request that is never sent.
fn build(endpoint: &HttpEndpoint, parts: Parts<'_>) -> Result<Request<Full<Bytes>>, HttpError> {
    let path = match parts.root {
        RouteRoot::Base => format!("{}{}", endpoint.base.0, parts.path),
        RouteRoot::Server => parts.path.to_owned(),
    };
    let accept = match parts.framing {
        Framing::Sse => "text/event-stream",
        Framing::Ndjson => "application/x-ndjson",
        Framing::Whole => "application/json",
    };
    let mut builder = Request::builder()
        .method(parts.method)
        .uri(path)
        .header(HOST, host_of(&endpoint.target))
        .header(ACCEPT, accept);
    if let Some(content_type) = parts.content_type {
        let value = HeaderValue::from_str(content_type).map_err(|_| HttpError::Connect)?;
        builder = builder.header(CONTENT_TYPE, value);
    }
    let secret = |name: WireName, value: &str| {
        HeaderValue::from_str(value)
            .map(|mut value| {
                value.set_sensitive(true);
                (name, value)
            })
            .map_err(|_| HttpError::Connect)
    };
    let mut headers = Vec::new();
    match &endpoint.auth {
        AuthHeader::None => {}
        AuthHeader::Bearer(token) => {
            headers.push(secret(AUTHORIZATION, &format!("Bearer {}", token.0))?);
        }
        AuthHeader::Header { name, value } => {
            let name = WireName::from_bytes(name.0.as_bytes()).map_err(|_| HttpError::Connect)?;
            headers.push(secret(name, &value.0)?);
        }
    }
    for extra in &endpoint.headers {
        let name = WireName::from_bytes(extra.name.0.as_bytes()).map_err(|_| HttpError::Connect)?;
        headers.push(secret(name, &extra.value.0)?);
    }
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    builder
        .body(Full::new(parts.body))
        .map_err(|_| HttpError::Connect)
}

fn host_of(target: &HttpTarget) -> String {
    match target {
        HttpTarget::Tcp { host, port } | HttpTarget::Tls { host, port } => {
            format!("{}:{}", host.0, port.0)
        }
        HttpTarget::Unix(_) => "localhost".to_owned(),
    }
}

/// One request on one connection: the handshake, then the exchange, while the connection's own
/// future is driven beside it. Neither is spawned, so dropping this future drops the socket.
async fn talk<IO, K>(
    io: IO,
    request: Request<Full<Bytes>>,
    timeouts: &Timeouts,
    sink: &mut K,
) -> Result<HttpStatus, HttpError>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    K: BodySink,
{
    let (mut sender, connection) = within(timeouts.connect, http1::handshake(TokioIo::new(io)))
        .await?
        .map_err(|_| HttpError::Connect)?;
    let work = respond(&mut sender, request, timeouts, sink);
    tokio::pin!(work);
    tokio::pin!(connection);
    let mut open = true;
    loop {
        tokio::select! {
            result = &mut work => return result,
            // The connection ending is not the exchange ending: the server may close after the
            // last byte while the body is still being read. Either way `work` finishes, with the
            // body or with the error the broken connection gives it.
            _ = &mut connection, if open => open = false,
        }
    }
}

async fn respond<K: BodySink>(
    sender: &mut http1::SendRequest<Full<Bytes>>,
    request: Request<Full<Bytes>>,
    timeouts: &Timeouts,
    sink: &mut K,
) -> Result<HttpStatus, HttpError> {
    let response = within(timeouts.first_byte, sender.send_request(request))
        .await?
        .map_err(|_| HttpError::Broken)?;
    let head = head_of(&response);
    let status = head.status;
    let mut body = response.into_body();
    let mut flow = sink.head(&head);
    while flow == ChunkFlow::Continue {
        match within(timeouts.idle, body.frame()).await? {
            None => break,
            Some(Err(_)) => return Err(HttpError::Broken),
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    flow = sink.chunk(&data);
                }
            }
        }
    }
    outcome(status)
}

/// A success is its status; anything else is `Rejected`, its head and body already in the sink.
fn outcome(status: HttpStatus) -> Result<HttpStatus, HttpError> {
    if (200..300).contains(&status.0) {
        Ok(status)
    } else {
        Err(HttpError::Rejected)
    }
}

fn head_of<B>(response: &Response<B>) -> ResponseHead {
    let text = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    ResponseHead {
        status: HttpStatus(response.status().as_u16()),
        body: BodyKind::of(&text(CONTENT_TYPE.as_str()).unwrap_or_default()),
        retry_after: text(RETRY_AFTER.as_str()).and_then(|v| WaitSeconds::from_header(&v)),
        request_id: text("x-request-id").and_then(|v| RequestId::new(v).ok()),
    }
}
