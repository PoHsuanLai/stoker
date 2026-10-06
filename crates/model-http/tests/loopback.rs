//! `Transport for HttpClient` against an in-process server on a loopback socket: the head arrives
//! before the first byte of the body, a non-success answer is `Rejected` with its body in the
//! sink, the request is what the endpoint says, and a stopped sink closes the connection.
//!
//! The server is a hand-written HTTP/1.1 peer on a Unix socket in a private directory or on
//! `127.0.0.1` with an ephemeral port; nothing leaves the machine.
#![cfg(feature = "hyper")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use model_http::{
    AuthHeader, BodyKind, BodySink, ChunkFlow, Exchange, ExtraHeader, Framing, HeaderName,
    HostName, HttpClient, HttpEndpoint, HttpError, HttpStatus, HttpTarget, JsonBody, Port, Proxy,
    ResponseHead, RouteRoot, Secret, Timeouts, Transport, UrlPath, Verb, WaitMs,
};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::oneshot;

/// What the server does after it has read the request.
enum Step {
    Write(Vec<u8>),
    Sleep(u64),
    /// Wait until the client's sink has seen the head.
    UntilHead,
    /// Write a chunk every 5 ms until the client closes the connection (or 400 chunks).
    Flood,
}

struct Server {
    target: HttpTarget,
    request: Arc<Mutex<String>>,
    peer_closed: Arc<AtomicBool>,
    /// Fired by the client's sink when it has the head; the server's `UntilHead` waits for it.
    head_signal: Option<oneshot::Sender<()>>,
}

fn scratch_socket() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "model-http-loopback-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("engine.sock")
}

#[derive(Clone, Copy)]
enum Wire {
    Unix,
    Tcp,
}

async fn serve(wire: Wire, steps: Vec<Step>) -> Server {
    let request = Arc::new(Mutex::new(String::new()));
    let peer_closed = Arc::new(AtomicBool::new(false));
    let (head_signal, head_seen) = oneshot::channel::<()>();
    let (req, closed) = (request.clone(), peer_closed.clone());
    let target = match wire {
        Wire::Unix => {
            let path = scratch_socket();
            let path_for_cleanup = path.clone();
            let listener = UnixListener::bind(&path).unwrap();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                // The client is connected: the scratch directory has done its job.
                if let Some(dir) = path_for_cleanup.parent() {
                    let _ = std::fs::remove_dir_all(dir);
                }
                handle(stream, steps, req, closed, head_seen).await;
            });
            HttpTarget::Unix(path)
        }
        Wire::Tcp => {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                handle(stream, steps, req, closed, head_seen).await;
            });
            HttpTarget::Tcp {
                host: HostName("127.0.0.1".into()),
                port: Port(port),
            }
        }
    };
    Server {
        target,
        request,
        peer_closed,
        head_signal: Some(head_signal),
    }
}

async fn handle<S>(
    mut stream: S,
    steps: Vec<Step>,
    request: Arc<Mutex<String>>,
    peer_closed: Arc<AtomicBool>,
    head_seen: oneshot::Receiver<()>,
) where
    S: tokio::io::AsyncRead + AsyncWrite + Unpin,
{
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
            let want = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if raw.len() >= end + 4 + want {
                break;
            }
        }
    }
    *request.lock().unwrap() = String::from_utf8_lossy(&raw).into_owned();
    let mut head_seen = Some(head_seen);
    for step in steps {
        match step {
            Step::Write(bytes) => {
                if stream.write_all(&bytes).await.is_err() || stream.flush().await.is_err() {
                    peer_closed.store(true, Ordering::SeqCst);
                    return;
                }
            }
            Step::Sleep(ms) => tokio::time::sleep(Duration::from_millis(ms)).await,
            Step::UntilHead => {
                if let Some(rx) = head_seen.take() {
                    let _ = rx.await;
                }
            }
            Step::Flood => {
                for _ in 0..400 {
                    let sent = stream.write_all(&chunk("data: x\n\n")).await;
                    if sent.is_err() || stream.flush().await.is_err() {
                        peer_closed.store(true, Ordering::SeqCst);
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        }
    }
}

fn chunk(data: &str) -> Vec<u8> {
    format!("{:x}\r\n{data}\r\n", data.len()).into_bytes()
}

fn end_chunks() -> Vec<u8> {
    b"0\r\n\r\n".to_vec()
}

fn head(status: &str, headers: &[&str]) -> Step {
    let mut text = format!("HTTP/1.1 {status}\r\n");
    for header in headers {
        text.push_str(header);
        text.push_str("\r\n");
    }
    text.push_str("\r\n");
    Step::Write(text.into_bytes())
}

fn sse_head() -> Step {
    head(
        "200 OK",
        &[
            "Content-Type: text/event-stream",
            "Transfer-Encoding: chunked",
        ],
    )
}

fn write(bytes: Vec<u8>) -> Step {
    Step::Write(bytes)
}

fn timeouts() -> Timeouts {
    Timeouts {
        connect: WaitMs(2_000),
        first_byte: WaitMs(5_000),
        idle: WaitMs(5_000),
    }
}

fn endpoint(target: HttpTarget) -> HttpEndpoint {
    HttpEndpoint {
        target,
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: timeouts(),
    }
}

fn post(path: &str, framing: Framing) -> Exchange {
    Exchange {
        verb: Verb::PostJson,
        root: RouteRoot::Base,
        path: UrlPath(path.into()),
        body: Some(JsonBody(r#"{"model":"holo"}"#.into())),
        framing,
    }
}

#[derive(Default)]
struct Seen {
    head: Option<ResponseHead>,
    chunks: Vec<Vec<u8>>,
    order: Vec<&'static str>,
    on_head: Option<oneshot::Sender<()>>,
    stop_after: Option<usize>,
}

impl Seen {
    fn body(&self) -> String {
        String::from_utf8_lossy(&self.chunks.concat()).into_owned()
    }
}

impl BodySink for Seen {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.order.push("head");
        self.head = Some(head.clone());
        if let Some(tx) = self.on_head.take() {
            let _ = tx.send(());
        }
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.order.push("chunk");
        self.chunks.push(bytes.to_vec());
        match self.stop_after {
            Some(n) if self.chunks.len() >= n => ChunkFlow::Stop,
            _ => ChunkFlow::Continue,
        }
    }
}

async fn run(
    endpoint: HttpEndpoint,
    ex: &Exchange,
    seen: &mut Seen,
) -> Result<HttpStatus, HttpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        HttpClient::new(endpoint).exchange(ex, seen),
    )
    .await
    .expect("the exchange hung")
}

#[tokio::test(flavor = "current_thread")]
async fn the_head_arrives_before_the_first_chunk_over_a_unix_socket_and_over_tcp() {
    for wire in [Wire::Unix, Wire::Tcp] {
        let mut server = serve(
            wire,
            vec![
                sse_head(),
                // The body is not sent until the client's sink has the head: a transport that
                // held the head back until a body chunk came would never get past this.
                Step::UntilHead,
                write(chunk("data: one\n\n")),
                Step::Sleep(20),
                write(chunk("data: two\n\n")),
                write(end_chunks()),
            ],
        )
        .await;
        let mut seen = Seen {
            on_head: server.head_signal.take(),
            ..Seen::default()
        };
        let ex = post("/chat/completions", Framing::Sse);
        let result = run(endpoint(server.target.clone()), &ex, &mut seen).await;
        assert_eq!(result, Ok(HttpStatus(200)));
        let head = seen.head.clone().unwrap();
        assert_eq!(
            (head.status, head.body, head.retry_after),
            (HttpStatus(200), BodyKind::EventStream, None)
        );
        assert_eq!(seen.order[0], "head");
        assert_eq!(seen.body(), "data: one\n\ndata: two\n\n");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_non_success_answer_is_rejected_with_its_head_and_body_in_the_sink() {
    let body = r#"{"error":{"message":"slow down","type":"rate_limit_error"}}"#;
    let server = serve(
        Wire::Unix,
        vec![
            head(
                "429 Too Many Requests",
                &[
                    "Content-Type: application/json",
                    "Retry-After: 7",
                    "X-Request-Id: req_abc",
                    &format!("Content-Length: {}", body.len()),
                ],
            ),
            write(body.as_bytes().to_vec()),
        ],
    )
    .await;
    let mut seen = Seen::default();
    let result = run(
        endpoint(server.target.clone()),
        &post("/chat/completions", Framing::Sse),
        &mut seen,
    )
    .await;
    assert_eq!(result, Err(HttpError::Rejected));
    let head = seen.head.unwrap();
    assert_eq!(head.status, HttpStatus(429));
    assert_eq!(head.body, BodyKind::Json);
    assert_eq!(head.retry_after, Some(model_http::WaitSeconds(7)));
    assert_eq!(head.request_id.unwrap().as_str(), "req_abc");
    assert_eq!(String::from_utf8(seen.chunks.concat()).unwrap(), body);
}

#[tokio::test(flavor = "current_thread")]
async fn an_html_page_is_labelled_html_whatever_its_status() {
    let page = "<html>Please sign in</html>";
    let server = serve(
        Wire::Tcp,
        vec![
            head(
                "200 OK",
                &[
                    "Content-Type: text/html; charset=utf-8",
                    &format!("Content-Length: {}", page.len()),
                ],
            ),
            write(page.as_bytes().to_vec()),
        ],
    )
    .await;
    let mut seen = Seen::default();
    let ex = Exchange {
        verb: Verb::Get,
        root: RouteRoot::Base,
        path: UrlPath("/models".into()),
        body: None,
        framing: Framing::Whole,
    };
    assert_eq!(
        run(endpoint(server.target.clone()), &ex, &mut seen).await,
        Ok(HttpStatus(200))
    );
    assert_eq!(seen.head.as_ref().unwrap().body, BodyKind::Html);
    assert_eq!(seen.body(), page);
}

#[tokio::test(flavor = "current_thread")]
async fn the_request_is_what_the_endpoint_says() {
    let server = serve(
        Wire::Unix,
        vec![
            head(
                "200 OK",
                &["Content-Type: application/json", "Content-Length: 2"],
            ),
            write(b"{}".to_vec()),
        ],
    )
    .await;
    let mut ep = endpoint(server.target.clone());
    ep.auth = AuthHeader::Bearer(Secret("sk-test-token".into()));
    ep.headers = vec![ExtraHeader {
        name: HeaderName("x-route".into()),
        value: Secret("local".into()),
    }];
    let mut seen = Seen::default();
    let result = run(ep, &post("/chat/completions", Framing::Sse), &mut seen).await;
    assert_eq!(result, Ok(HttpStatus(200)));
    let raw = server.request.lock().unwrap().clone();
    let lower = raw.to_ascii_lowercase();
    assert!(
        raw.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"),
        "{raw}"
    );
    assert!(lower.contains("\r\nauthorization: bearer sk-test-token\r\n"));
    assert!(lower.contains("\r\nx-route: local\r\n"));
    assert!(lower.contains("\r\ncontent-type: application/json\r\n"));
    assert!(lower.contains("\r\naccept: text/event-stream\r\n"));
    assert!(lower.contains("\r\nhost: localhost\r\n"));
    assert!(raw.ends_with("\r\n\r\n{\"model\":\"holo\"}"));
}

#[tokio::test(flavor = "current_thread")]
async fn a_route_at_the_server_root_skips_the_base_and_a_vendor_header_authenticates() {
    let server = serve(
        Wire::Unix,
        vec![
            head(
                "200 OK",
                &["Content-Type: application/json", "Content-Length: 2"],
            ),
            write(b"{}".to_vec()),
        ],
    )
    .await;
    let mut ep = endpoint(server.target.clone());
    ep.auth = AuthHeader::Header {
        name: HeaderName("x-api-key".into()),
        value: Secret("k".into()),
    };
    let ex = Exchange {
        verb: Verb::Get,
        root: RouteRoot::Server,
        path: UrlPath("/props".into()),
        body: None,
        framing: Framing::Whole,
    };
    assert_eq!(
        run(ep, &ex, &mut Seen::default()).await,
        Ok(HttpStatus(200))
    );
    let raw = server.request.lock().unwrap().to_ascii_lowercase();
    assert!(raw.starts_with("get /props http/1.1\r\n"), "{raw}");
    assert!(raw.contains("\r\nx-api-key: k\r\n"));
    assert!(!raw.contains("authorization"));
    assert!(raw.contains("\r\naccept: application/json\r\n"));
    assert!(!raw.contains("content-type"), "a GET has no body type");
}

#[tokio::test(flavor = "current_thread")]
async fn ndjson_is_asked_for_and_labelled() {
    let server = serve(
        Wire::Unix,
        vec![
            head(
                "200 OK",
                &[
                    "Content-Type: application/x-ndjson",
                    "Transfer-Encoding: chunked",
                ],
            ),
            write(chunk("{\"a\":1}\n")),
            write(end_chunks()),
        ],
    )
    .await;
    let mut seen = Seen::default();
    let result = run(
        endpoint(server.target.clone()),
        &post("/api/chat", Framing::Ndjson),
        &mut seen,
    )
    .await;
    assert_eq!(result, Ok(HttpStatus(200)));
    assert_eq!(seen.head.unwrap().body, BodyKind::NdJson);
    assert!(
        server
            .request
            .lock()
            .unwrap()
            .to_ascii_lowercase()
            .contains("accept: application/x-ndjson")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_sink_that_stops_closes_the_connection() {
    let server = serve(Wire::Unix, vec![sse_head(), Step::Flood]).await;
    let mut seen = Seen {
        stop_after: Some(2),
        ..Seen::default()
    };
    let result = run(
        endpoint(server.target.clone()),
        &post("/chat/completions", Framing::Sse),
        &mut seen,
    )
    .await;
    assert_eq!(
        result,
        Ok(HttpStatus(200)),
        "an early stop is not a failure"
    );
    assert_eq!(seen.chunks.len(), 2, "nothing is read after the stop");
    for _ in 0..200 {
        if server.peer_closed.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the server never saw the connection close");
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_the_future_closes_the_connection() {
    let server = serve(Wire::Tcp, vec![sse_head(), Step::Flood]).await;
    let client = HttpClient::new(endpoint(server.target.clone()));
    let ex = post("/chat/completions", Framing::Sse);
    let mut seen = Seen::default();
    let _ = tokio::time::timeout(Duration::from_millis(100), client.exchange(&ex, &mut seen)).await;
    assert!(!seen.chunks.is_empty(), "some of the stream was read first");
    for _ in 0..200 {
        if server.peer_closed.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the server never saw the connection close");
}

#[tokio::test(flavor = "current_thread")]
async fn a_connection_cut_mid_body_is_broken_after_the_chunks_that_arrived() {
    let server = serve(
        Wire::Unix,
        vec![sse_head(), write(chunk("data: one\n\n")), Step::Sleep(20)],
    )
    .await;
    let mut seen = Seen::default();
    let result = run(
        endpoint(server.target.clone()),
        &post("/chat/completions", Framing::Sse),
        &mut seen,
    )
    .await;
    assert_eq!(result, Err(HttpError::Broken));
    assert_eq!(seen.body(), "data: one\n\n");
}

#[tokio::test(flavor = "current_thread")]
async fn a_server_that_closes_after_the_last_byte_is_still_a_complete_reply() {
    let body = "data: one\n\n";
    let server = serve(
        Wire::Unix,
        vec![
            head(
                "200 OK",
                &["Content-Type: text/event-stream", "Connection: close"],
            ),
            write(body.as_bytes().to_vec()),
        ],
    )
    .await;
    let mut seen = Seen::default();
    let result = run(
        endpoint(server.target.clone()),
        &post("/chat/completions", Framing::Sse),
        &mut seen,
    )
    .await;
    assert_eq!(result, Ok(HttpStatus(200)));
    assert_eq!(seen.body(), body);
}

#[tokio::test(flavor = "current_thread")]
async fn the_timeouts_are_the_endpoints() {
    // A server that accepts and never answers: the first byte does not come.
    let server = serve(Wire::Unix, vec![Step::Sleep(3_000)]).await;
    let mut ep = endpoint(server.target.clone());
    ep.timeouts.first_byte = WaitMs(100);
    let result = run(
        ep,
        &post("/chat/completions", Framing::Sse),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Timeout));

    // A head and then nothing: the idle timeout between chunks.
    let server = serve(
        Wire::Unix,
        vec![
            sse_head(),
            write(chunk("data: one\n\n")),
            Step::Sleep(3_000),
        ],
    )
    .await;
    let mut ep = endpoint(server.target.clone());
    ep.timeouts.idle = WaitMs(100);
    let mut seen = Seen::default();
    let result = run(ep, &post("/chat/completions", Framing::Sse), &mut seen).await;
    assert_eq!(result, Err(HttpError::Timeout));
    assert_eq!(seen.chunks.len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn nothing_listening_is_connect() {
    let gone = scratch_socket();
    std::fs::remove_dir_all(gone.parent().unwrap()).unwrap();
    let result = run(
        endpoint(HttpTarget::Unix(gone)),
        &post("/chat/completions", Framing::Sse),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Connect));

    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };
    let target = HttpTarget::Tcp {
        host: HostName("127.0.0.1".into()),
        port: Port(port),
    };
    let result = run(
        endpoint(target),
        &post("/chat/completions", Framing::Sse),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Connect));
}

// Without the `tls` feature a Tls target is refused before anything is connected (the name does
// not resolve here, so a connect attempt would be the network). With it, tests/tls.rs covers it.
#[cfg(not(feature = "tls"))]
#[tokio::test(flavor = "current_thread")]
async fn tls_without_the_feature_sends_nothing() {
    let tls = HttpTarget::Tls {
        host: HostName("api.example.com".into()),
        port: Port(443),
    };
    let result = run(
        endpoint(tls),
        &post("/chat/completions", Framing::Sse),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
}

#[tokio::test(flavor = "current_thread")]
async fn a_proxy_is_not_built_and_sends_nothing() {
    for target in [
        HttpTarget::Tls {
            host: HostName("api.example.com".into()),
            port: Port(443),
        },
        HttpTarget::Tcp {
            host: HostName("api.example.com".into()),
            port: Port(80),
        },
    ] {
        let mut proxied = endpoint(target);
        proxied.proxy = Proxy::Via(Box::new(HttpTarget::Tcp {
            host: HostName("127.0.0.1".into()),
            port: Port(3128),
        }));
        let result = run(
            proxied,
            &post("/chat/completions", Framing::Sse),
            &mut Seen::default(),
        )
        .await;
        assert_eq!(result, Err(HttpError::Connect));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_header_that_cannot_be_written_is_a_request_never_sent() {
    let server = serve(Wire::Unix, vec![]).await;
    let mut ep = endpoint(server.target.clone());
    ep.auth = AuthHeader::Bearer(Secret("bad\ntoken".into()));
    let result = run(
        ep,
        &post("/chat/completions", Framing::Sse),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Connect));
    assert!(server.request.lock().unwrap().is_empty());
    // Nobody connected, so the server never cleaned up after itself.
    if let HttpTarget::Unix(path) = &server.target {
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
