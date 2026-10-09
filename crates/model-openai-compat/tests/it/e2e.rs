//! The whole stack once: `OpenAiCodec` and `Driver` over `HttpClient` against a loopback server
//! that speaks the chat-completions stream on a Unix socket.

use crate::support;

use std::time::Duration;

use model_http::{
    AuthHeader, HttpClient, HttpEndpoint, HttpTarget, Proxy, Timeouts, UrlPath, WaitMs,
};
use model_openai_compat::{Flavor, OpenAiCodec};
use model_provider::{
    Flow, ModelName, Provider, ProviderError, RetrySeconds, StopReason, TurnEvent, TurnSink,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

#[derive(Default)]
struct Keep(Vec<TurnEvent>);

impl TurnSink for Keep {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.0.push(event);
        Flow::Continue
    }
}

fn endpoint(path: std::path::PathBuf) -> HttpEndpoint {
    HttpEndpoint {
        target: HttpTarget::Unix(path),
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

/// Answers one request with `response`, in `pieces` written a few milliseconds apart; returns
/// the request it read.
async fn serve_once(
    response: Vec<Vec<u8>>,
) -> (std::path::PathBuf, tokio::task::JoinHandle<String>) {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("openai-compat-e2e-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("engine.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            raw.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&raw).into_owned();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let want = head
                    .to_ascii_lowercase()
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: ").map(str::to_owned))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if body.len() >= want {
                    break;
                }
            }
        }
        for piece in response {
            stream.write_all(&piece).await.unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
        String::from_utf8_lossy(&raw).into_owned()
    });
    (path, task)
}

fn chunked(data: &str) -> Vec<u8> {
    format!("{:x}\r\n{data}\r\n", data.len()).into_bytes()
}

#[tokio::test(flavor = "current_thread")]
async fn a_turn_streams_over_a_unix_socket_through_the_driver() {
    let frame = |delta: &str, finish: &str| {
        format!(
            "data: {{\"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish}}}]}}\n\n"
        )
    };
    let pieces = vec![
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n"
            .to_vec(),
        chunked(&frame(r#"{"content":"Hel"}"#, "null")),
        chunked(&frame(r#"{"content":"lo"}"#, "null")),
        chunked(&frame("{}", r#""stop""#)),
        chunked(
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n",
        ),
        b"0\r\n\r\n".to_vec(),
    ];
    let (path, server) = serve_once(pieces).await;
    let provider = OpenAiCodec::new(Flavor::Vllm).provider(HttpClient::new(endpoint(path)));
    let mut request = support::base();
    request.messages = vec![support::user("hi")];
    let mut sink = Keep::default();
    let end = provider.turn(&request, &mut sink).await.unwrap();
    assert_eq!(end.stop, StopReason::EndTurn);
    assert_eq!(end.served, ModelName("holo".into()));
    assert_eq!((end.usage.input.0, end.usage.output.0), (5, 2));
    assert_eq!(
        sink.0[..2],
        [
            TurnEvent::TextDelta("Hel".into()),
            TurnEvent::TextDelta("lo".into())
        ]
    );
    let raw = server.await.unwrap();
    assert!(raw.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert!(raw.contains("\"stream\":true"));
    assert!(raw.contains("\"content\":\"hi\""));
}

#[tokio::test(flavor = "current_thread")]
async fn a_rate_limit_over_the_socket_carries_its_retry_after() {
    let body = r#"{"error":{"type":"rate_limit_error","message":"slow"}}"#;
    let pieces = vec![format!(
        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 4\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()];
    let (path, _server) = serve_once(pieces).await;
    let provider = OpenAiCodec::new(Flavor::Vllm).provider(HttpClient::new(endpoint(path)));
    let mut request = support::base();
    request.messages = vec![support::user("hi")];
    let result = provider.turn(&request, &mut Keep::default()).await;
    assert_eq!(result, Err(ProviderError::RateLimited(RetrySeconds(4))));
}

#[tokio::test(flavor = "current_thread")]
async fn describe_reads_the_models_over_the_socket() {
    let body = r#"{"data":[{"id":"holo","max_model_len":16384}]}"#;
    let pieces =
        vec![format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()];
    let (path, server) = serve_once(pieces).await;
    let provider = OpenAiCodec::new(Flavor::Vllm).provider(HttpClient::new(endpoint(path)));
    let models = provider.describe().await.unwrap();
    assert_eq!(models[0].loaded_context.0, 16384);
    assert!(
        server
            .await
            .unwrap()
            .starts_with("GET /v1/models HTTP/1.1\r\n")
    );
}
