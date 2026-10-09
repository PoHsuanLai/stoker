//! `UploadTransport for HttpClient` against a hand-written HTTP/1.1 peer on a Unix socket in a
//! private directory: the bytes of the body arrive exactly, the headers say what they are, the
//! reply reaches the sink, and a refusal is `Rejected` with its body in the sink.
#![cfg(feature = "hyper")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use model_http::{
    AuthHeader, BodySink, ChunkFlow, ContentType, Framing, HttpClient, HttpEndpoint, HttpError,
    HttpStatus, HttpTarget, Proxy, RawBody, ResponseHead, RouteRoot, Secret, Timeouts, Upload,
    UploadTransport, UrlPath, WaitMs,
};
use proptest::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// The request exactly as it came: the head text and the body bytes.
#[derive(Debug, Default, Clone)]
struct Request {
    head: String,
    body: Vec<u8>,
}

fn scratch() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "model-http-upload-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("engine.sock")
}

/// Serves one request: reads it whole, then writes `reply`.
fn serve(reply: Vec<u8>) -> (HttpTarget, Arc<Mutex<Request>>) {
    let path = scratch();
    let listener = UnixListener::bind(&path).unwrap();
    let seen = Arc::new(Mutex::new(Request::default()));
    let (kept, cleanup) = (seen.clone(), path.clone());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        if let Some(dir) = cleanup.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        let (head_end, want) = loop {
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
                    break (end, want);
                }
            }
        };
        *kept.lock().unwrap() = Request {
            head: String::from_utf8_lossy(&raw[..head_end]).into_owned(),
            body: raw[head_end + 4..head_end + 4 + want].to_vec(),
        };
        let _ = stream.write_all(&reply).await;
        let _ = stream.flush().await;
    });
    (HttpTarget::Unix(path), seen)
}

fn answer(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn endpoint(target: HttpTarget, auth: AuthHeader) -> HttpEndpoint {
    HttpEndpoint {
        target,
        proxy: Proxy::Direct,
        base: UrlPath("/v1".into()),
        auth,
        headers: vec![],
        timeouts: Timeouts {
            connect: WaitMs(2_000),
            first_byte: WaitMs(5_000),
            idle: WaitMs(5_000),
        },
    }
}

fn upload(content_type: &str, bytes: Vec<u8>) -> Upload {
    Upload {
        root: RouteRoot::Base,
        path: UrlPath("/audio/transcriptions".into()),
        body: RawBody {
            content_type: ContentType(content_type.into()),
            bytes,
        },
        framing: Framing::Whole,
    }
}

#[derive(Default)]
struct Seen {
    head: Option<ResponseHead>,
    body: Vec<u8>,
}

impl BodySink for Seen {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.head = Some(head.clone());
        ChunkFlow::Continue
    }
    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.body.extend_from_slice(bytes);
        ChunkFlow::Continue
    }
}

async fn run(
    endpoint: HttpEndpoint,
    up: &Upload,
    seen: &mut Seen,
) -> Result<HttpStatus, HttpError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        HttpClient::new(endpoint).upload(up, seen),
    )
    .await
    .expect("the upload finished")
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}: ");
    request.head.lines().find_map(|l| {
        l.to_ascii_lowercase()
            .starts_with(&prefix)
            .then(|| &l[prefix.len()..])
    })
}

#[tokio::test]
async fn the_bytes_arrive_exactly_under_the_content_type_that_names_them() {
    let (target, request) = serve(answer("200 OK", "application/json", r#"{"text":"hi"}"#));
    // Bytes that are not text, a CRLF pair, a boundary look-alike, a zero.
    let body: Vec<u8> = [
        &b"--b\r\n\r\n"[..],
        &[0, 255, 254, 13, 10, 0x80],
        b"--b--\r\n",
    ]
    .concat();
    let mut seen = Seen::default();
    let ct = "multipart/form-data; boundary=b";
    let status = run(
        endpoint(target, AuthHeader::Bearer(Secret("tok".into()))),
        &upload(ct, body.clone()),
        &mut seen,
    )
    .await;
    assert_eq!(status, Ok(HttpStatus(200)));
    let request = request.lock().unwrap().clone();
    assert!(
        request
            .head
            .starts_with("POST /v1/audio/transcriptions HTTP/1.1"),
        "{}",
        request.head
    );
    assert_eq!(header(&request, "content-type"), Some(ct));
    assert_eq!(
        header(&request, "content-length"),
        Some(body.len().to_string().as_str())
    );
    assert_eq!(header(&request, "authorization"), Some("Bearer tok"));
    assert_eq!(header(&request, "accept"), Some("application/json"));
    assert_eq!(request.body, body);
    assert_eq!(seen.head.unwrap().status, HttpStatus(200));
    assert_eq!(seen.body, br#"{"text":"hi"}"#);
}

#[tokio::test]
async fn the_server_root_path_skips_the_base() {
    let (target, request) = serve(answer("200 OK", "application/json", "{}"));
    let mut up = upload("application/octet-stream", vec![1, 2, 3]);
    up.root = RouteRoot::Server;
    up.path = UrlPath("/upload".into());
    run(
        endpoint(target, AuthHeader::None),
        &up,
        &mut Seen::default(),
    )
    .await
    .unwrap();
    let head = request.lock().unwrap().head.clone();
    assert!(head.starts_with("POST /upload HTTP/1.1"), "{head}");
}

#[tokio::test]
async fn a_refusal_is_rejected_with_its_head_and_body_in_the_sink() {
    let (target, _) = serve(answer(
        "429 Too Many Requests",
        "application/json",
        r#"{"error":"slow down"}"#,
    ));
    let mut seen = Seen::default();
    let result = run(
        endpoint(target, AuthHeader::None),
        &upload("audio/wav", vec![0; 16]),
        &mut seen,
    )
    .await;
    assert_eq!(result, Err(HttpError::Rejected));
    assert_eq!(seen.head.unwrap().status, HttpStatus(429));
    assert_eq!(seen.body, br#"{"error":"slow down"}"#);
}

#[tokio::test]
async fn nobody_listening_is_connect_and_a_bad_content_type_is_never_sent() {
    let nowhere = HttpTarget::Unix(scratch());
    let mut seen = Seen::default();
    assert_eq!(
        run(
            endpoint(nowhere, AuthHeader::None),
            &upload("audio/wav", vec![1]),
            &mut seen
        )
        .await,
        Err(HttpError::Connect)
    );
    let (target, request) = serve(answer("200 OK", "application/json", "{}"));
    let bad = upload("audio/wav\r\nX-Injected: 1", vec![1]);
    assert_eq!(
        run(endpoint(target, AuthHeader::None), &bad, &mut seen).await,
        Err(HttpError::Connect)
    );
    assert!(request.lock().unwrap().head.is_empty());
    assert!(seen.head.is_none());
}

#[test]
fn any_bytes_in_any_size_arrive_unchanged() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(24));
    runner
        .run(
            &proptest::collection::vec(any::<u8>(), 0..70_000),
            |bytes| {
                runtime.block_on(async {
                    let (target, request) = serve(answer("200 OK", "application/json", "{}"));
                    let status = run(
                        endpoint(target, AuthHeader::None),
                        &upload("application/octet-stream", bytes.clone()),
                        &mut Seen::default(),
                    )
                    .await;
                    prop_assert_eq!(status, Ok(HttpStatus(200)));
                    prop_assert_eq!(&request.lock().unwrap().body, &bytes);
                    Ok(())
                })
            },
        )
        .unwrap();
}
