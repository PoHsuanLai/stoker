//! `Transport for HttpClient` over reqwest: HTTP/1.1 on a TCP or Unix socket, one connection per
//! exchange (dropping the future drops the connection, which aborts generation in the engine).
//!
//! A `Tls` target is TLS over TCP behind the `tls` feature (see `connect.rs`); without the
//! feature it answers `HttpError::Tls` without sending anything. The egress proxy is not built
//! yet: a `Via` proxy answers `HttpError::Connect` for every target, `Tls` included, without
//! sending anything.
//!
//! The stages keep their timeouts. The connect timeout is the client's (it covers the TCP connect
//! and the TLS handshake); the head must arrive within the connect and first-byte timeouts
//! together, because reqwest hands over the response only once the head is read; the idle timeout
//! runs between chunks of the body. The sink's `deadline` bounds the whole exchange, connect
//! included.

use std::future::Future;
use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, Response};
use tokio::time::{sleep, timeout};

use crate::connect::{is_tls_failure, millis};
use crate::request::{Parts, Prepared, prepare};
use crate::{
    BodyKind, BodySink, ChunkFlow, Exchange, HttpClient, HttpError, HttpStatus, HttpTarget,
    Progress, Proxy, RequestId, ResponseHead, Timeouts, Transport, Upload, UploadTransport, WaitMs,
    WaitSeconds,
};

impl Transport for HttpClient {
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        send(self, Parts::of_exchange(ex), sink)
    }
}

impl UploadTransport for HttpClient {
    fn upload<K: BodySink>(
        &self,
        up: &Upload,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        send(self, Parts::of_upload(up), sink)
    }
}

/// Writes the request, connects, and reads the response into `sink`.
async fn send<K: BodySink>(
    client: &HttpClient,
    parts: Parts<'_>,
    sink: &mut K,
) -> Result<HttpStatus, HttpError> {
    let endpoint = client.endpoint();
    let request = prepare(endpoint, parts)?;
    if matches!(endpoint.proxy, Proxy::Via(_)) {
        return Err(HttpError::Connect);
    }
    let http = client.wire().get(endpoint, client.roots())?;
    bounded(
        sink.deadline(),
        talk(http, &endpoint.target, request, &endpoint.timeouts, sink),
    )
    .await
}

/// Runs `future` for at most `wait`; an overrun is `HttpError::Timeout`.
async fn within<T>(wait: Duration, future: impl Future<Output = T>) -> Result<T, HttpError> {
    timeout(wait, future).await.map_err(|_| HttpError::Timeout)
}

/// Runs `future` within `total` when there is one.
async fn bounded<T>(
    total: Option<WaitMs>,
    future: impl Future<Output = Result<T, HttpError>>,
) -> Result<T, HttpError> {
    match total {
        Some(wait) => within(millis(wait), future).await?,
        None => future.await,
    }
}

/// What a failure of the request before the head came back means.
fn refused(error: &reqwest::Error, target: &HttpTarget) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout
    } else if !error.is_connect() {
        HttpError::Broken
    } else if matches!(target, HttpTarget::Tls { .. }) && is_tls_failure(error) {
        HttpError::Tls
    } else {
        HttpError::Connect
    }
}

async fn talk<K: BodySink>(
    http: &Client,
    target: &HttpTarget,
    request: Prepared,
    timeouts: &Timeouts,
    sink: &mut K,
) -> Result<HttpStatus, HttpError> {
    let pending = http
        .request(request.method, request.url)
        .headers(request.headers)
        .body(request.body)
        .send();
    let head_within = millis(timeouts.connect).saturating_add(millis(timeouts.first_byte));
    let mut response = within(head_within, pending)
        .await?
        .map_err(|e| refused(&e, target))?;
    let head = head_of(&response);
    let status = head.status;
    let mut flow = sink.head(&head);
    let idle = millis(timeouts.idle);
    // The idle timer restarts only when a chunk made progress, so keep-alive comments and empty
    // events do not hold the turn open.
    let mut timer = Box::pin(sleep(idle));
    while flow == ChunkFlow::Continue {
        let next = tokio::select! {
            () = &mut timer => return Err(HttpError::Timeout),
            next = response.chunk() => next,
        };
        match next {
            Ok(None) => break,
            Err(_) => return Err(HttpError::Broken),
            Ok(Some(data)) => {
                flow = sink.chunk(&data);
                if sink.progress() == Progress::Made {
                    timer = Box::pin(sleep(idle));
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

fn head_of(response: &Response) -> ResponseHead {
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
        location: text("location"),
        content_range: text("content-range"),
    }
}
