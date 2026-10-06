//! `Transport for HttpClient` over TLS against an in-process rustls server on `127.0.0.1`, with a
//! scratch CA made by rcgen. Nothing leaves the machine. The client trusts the scratch CA through
//! `TlsRoots::Only`; every other case shows that the checks cannot be skipped.
#![cfg(feature = "tls")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use model_http::{
    AuthHeader, BodySink, ChunkFlow, DerCertificate, Exchange, Framing, HostName, HttpClient,
    HttpEndpoint, HttpError, HttpStatus, HttpTarget, JsonBody, Port, Proxy, ResponseHead,
    RouteRoot, Timeouts, TlsRoots, Transport, UrlPath, Verb, WaitMs,
};
use rcgen::{BasicConstraints, Certificate, CertificateParams, IsCa, KeyPair, date_time_ymd};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring::default_provider;

struct Ca {
    cert: Certificate,
    key: KeyPair,
}

impl Ca {
    fn new() -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let key = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        Self { cert, key }
    }

    fn roots(&self) -> TlsRoots {
        TlsRoots::Only(vec![DerCertificate(self.cert.der().to_vec())])
    }

    /// A leaf for `names`, valid until `expiry` (a year, or the past).
    fn leaf(&self, names: &[&str], expiry: Option<i32>) -> (CertificateDer<'static>, KeyPair) {
        let mut params =
            CertificateParams::new(names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>())
                .unwrap();
        if let Some(year) = expiry {
            params.not_before = date_time_ymd(year - 2, 1, 1);
            params.not_after = date_time_ymd(year, 1, 1);
        }
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &self.cert, &self.key).unwrap();
        (cert.der().clone(), key)
    }
}

/// What the server does once the request is read.
enum Step {
    Write(Vec<u8>),
    Sleep(u64),
}

#[derive(Default)]
struct Handshake {
    sni: Option<String>,
    alpn: Option<Vec<u8>>,
    request: String,
}

struct Server {
    port: u16,
    seen: Arc<Mutex<Handshake>>,
}

/// A TLS server presenting `leaf`, running `steps` for one connection. `None` is a peer that
/// accepts the TCP connection and says nothing.
async fn serve(leaf: Option<(CertificateDer<'static>, KeyPair)>, steps: Vec<Step>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Handshake::default()));
    let record = seen.clone();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let Some((cert, key)) = leaf else {
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(tcp);
            return;
        };
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let mut config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let Ok(mut tls) = TlsAcceptor::from(Arc::new(config)).accept(tcp).await else {
            return;
        };
        {
            let (_, conn) = tls.get_ref();
            let mut seen = record.lock().unwrap();
            seen.sni = conn.server_name().map(str::to_owned);
            seen.alpn = conn.alpn_protocol().map(<[u8]>::to_vec);
        }
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
            match tls.read(&mut buf).await {
                Ok(n) if n > 0 => raw.extend_from_slice(&buf[..n]),
                _ => return,
            }
        }
        record.lock().unwrap().request = String::from_utf8_lossy(&raw).into_owned();
        for step in steps {
            match step {
                Step::Write(bytes) => {
                    if tls.write_all(&bytes).await.is_err() || tls.flush().await.is_err() {
                        return;
                    }
                }
                Step::Sleep(ms) => tokio::time::sleep(Duration::from_millis(ms)).await,
            }
        }
        let _ = tls.shutdown().await;
    });
    Server { port, seen }
}

fn chunk(data: &str) -> Vec<u8> {
    format!("{:x}\r\n{data}\r\n", data.len()).into_bytes()
}

fn sse_head() -> Step {
    Step::Write(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n"
            .to_vec(),
    )
}

fn endpoint(host: &str, port: u16) -> HttpEndpoint {
    HttpEndpoint {
        target: HttpTarget::Tls {
            host: HostName(host.into()),
            port: Port(port),
        },
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth: AuthHeader::None,
        headers: vec![],
        timeouts: Timeouts {
            connect: WaitMs(2_000),
            first_byte: WaitMs(5_000),
            idle: WaitMs(5_000),
        },
    }
}

fn post() -> Exchange {
    Exchange {
        verb: Verb::PostJson,
        root: RouteRoot::Base,
        path: UrlPath("/chat/completions".into()),
        body: Some(JsonBody(r#"{"model":"holo"}"#.into())),
        framing: Framing::Sse,
    }
}

#[derive(Default)]
struct Seen {
    chunks: Vec<Vec<u8>>,
}

impl Seen {
    fn body(&self) -> String {
        String::from_utf8_lossy(&self.chunks.concat()).into_owned()
    }
}

impl BodySink for Seen {
    fn head(&mut self, _: &ResponseHead) -> ChunkFlow {
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.chunks.push(bytes.to_vec());
        ChunkFlow::Continue
    }
}

async fn run(ep: HttpEndpoint, roots: TlsRoots, seen: &mut Seen) -> Result<HttpStatus, HttpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        HttpClient::with_roots(ep, roots).exchange(&post(), seen),
    )
    .await
    .expect("the exchange hung")
}

fn ok_steps() -> Vec<Step> {
    vec![Step::Write(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"
            .to_vec(),
    )]
}

#[tokio::test(flavor = "current_thread")]
async fn a_request_succeeds_when_the_scratch_ca_is_trusted() {
    let ca = Ca::new();
    let server = serve(Some(ca.leaf(&["localhost"], None)), ok_steps()).await;
    let mut seen = Seen::default();
    let result = run(endpoint("localhost", server.port), ca.roots(), &mut seen).await;
    assert_eq!(result, Ok(HttpStatus(200)));
    assert_eq!(seen.body(), "{}");
    let hs = server.seen.lock().unwrap();
    assert_eq!(hs.sni.as_deref(), Some("localhost"));
    assert_eq!(hs.alpn.as_deref(), Some(&b"http/1.1"[..]));
    assert!(
        hs.request
            .starts_with("POST /v1/chat/completions HTTP/1.1\r\n")
    );
    assert!(
        hs.request
            .to_ascii_lowercase()
            .contains(&format!("\r\nhost: localhost:{}\r\n", server.port))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn sse_streams_over_tls() {
    let ca = Ca::new();
    let server = serve(
        Some(ca.leaf(&["localhost"], None)),
        vec![
            sse_head(),
            Step::Write(chunk("data: one\n\n")),
            Step::Sleep(20),
            Step::Write(chunk("data: two\n\n")),
            Step::Write(b"0\r\n\r\n".to_vec()),
        ],
    )
    .await;
    let mut seen = Seen::default();
    let result = run(endpoint("localhost", server.port), ca.roots(), &mut seen).await;
    assert_eq!(result, Ok(HttpStatus(200)));
    assert_eq!(seen.body(), "data: one\n\ndata: two\n\n");
}

#[tokio::test(flavor = "current_thread")]
async fn a_server_signed_by_another_ca_is_a_tls_error() {
    let (real, other) = (Ca::new(), Ca::new());
    let server = serve(Some(real.leaf(&["localhost"], None)), ok_steps()).await;
    let mut seen = Seen::default();
    let result = run(endpoint("localhost", server.port), other.roots(), &mut seen).await;
    assert_eq!(result, Err(HttpError::Tls));
    assert!(seen.chunks.is_empty());
    assert!(server.seen.lock().unwrap().request.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn the_platform_roots_do_not_trust_a_scratch_ca() {
    let ca = Ca::new();
    let server = serve(Some(ca.leaf(&["localhost"], None)), ok_steps()).await;
    let result = run(
        endpoint("localhost", server.port),
        TlsRoots::Platform,
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
}

#[tokio::test(flavor = "current_thread")]
async fn a_certificate_for_another_name_is_a_tls_error() {
    let ca = Ca::new();
    let server = serve(Some(ca.leaf(&["other.example"], None)), ok_steps()).await;
    let result = run(
        endpoint("localhost", server.port),
        ca.roots(),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
    assert!(server.seen.lock().unwrap().request.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn an_expired_certificate_is_a_tls_error() {
    let ca = Ca::new();
    let server = serve(Some(ca.leaf(&["localhost"], Some(2020))), ok_steps()).await;
    let result = run(
        endpoint("localhost", server.port),
        ca.roots(),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
}

#[tokio::test(flavor = "current_thread")]
async fn a_name_that_is_not_a_dns_name_is_a_tls_error() {
    let ca = Ca::new();
    let server = serve(Some(ca.leaf(&["localhost"], None)), ok_steps()).await;
    let result = run(
        endpoint("not a host name", server.port),
        ca.roots(),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
}

#[tokio::test(flavor = "current_thread")]
async fn a_plain_http_server_is_a_tls_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut tcp, _) = listener.accept().await.unwrap();
        let _ = tcp
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .await;
        let _ = tcp.shutdown().await;
    });
    let result = run(
        endpoint("localhost", port),
        Ca::new().roots(),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Tls));
}

#[tokio::test(flavor = "current_thread")]
async fn nothing_listening_is_connect() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    };
    let result = run(
        endpoint("localhost", port),
        Ca::new().roots(),
        &mut Seen::default(),
    )
    .await;
    assert_eq!(result, Err(HttpError::Connect));
}

#[tokio::test(flavor = "current_thread")]
async fn the_timeouts_are_the_endpoints() {
    let ca = Ca::new();

    // A peer that accepts and never starts the handshake: the connect timeout covers it.
    let server = serve(None, vec![]).await;
    let mut ep = endpoint("localhost", server.port);
    ep.timeouts.connect = WaitMs(100);
    let result = run(ep, ca.roots(), &mut Seen::default()).await;
    assert_eq!(result, Err(HttpError::Timeout));

    // A finished handshake and then nothing: the first byte does not come.
    let server = serve(
        Some(ca.leaf(&["localhost"], None)),
        vec![Step::Sleep(3_000)],
    )
    .await;
    let mut ep = endpoint("localhost", server.port);
    ep.timeouts.first_byte = WaitMs(100);
    let result = run(ep, ca.roots(), &mut Seen::default()).await;
    assert_eq!(result, Err(HttpError::Timeout));

    // A head and a chunk, then nothing: the idle timeout between chunks.
    let server = serve(
        Some(ca.leaf(&["localhost"], None)),
        vec![
            sse_head(),
            Step::Write(chunk("data: one\n\n")),
            Step::Sleep(3_000),
        ],
    )
    .await;
    let mut ep = endpoint("localhost", server.port);
    ep.timeouts.idle = WaitMs(100);
    let mut seen = Seen::default();
    let result = run(ep, ca.roots(), &mut seen).await;
    assert_eq!(result, Err(HttpError::Timeout));
    assert_eq!(seen.chunks.len(), 1);
}
