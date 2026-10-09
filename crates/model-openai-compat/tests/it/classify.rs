//! `classify`: the status class first, then the envelope; no arm carries the body.

use model_http::{BodyKind, HttpStatus, ResponseHead, WaitSeconds};
use model_openai_compat::{Flavor, OpenAiCodec};
use model_provider::{ProviderDetail, ProviderError, RetrySeconds, ServerStatus, Tokens};
use model_wire::ErrorWire;

fn head(status: u16, body: BodyKind, retry_after: Option<u32>) -> ResponseHead {
    ResponseHead {
        status: HttpStatus(status),
        body,
        retry_after: retry_after.map(WaitSeconds),
        request_id: None,
    }
}

fn classify(status: u16, body: &str) -> ProviderError {
    OpenAiCodec::new(Flavor::Vllm).classify(&head(status, BodyKind::Json, None), body.as_bytes())
}

fn bad(slug: &str) -> ProviderError {
    ProviderError::BadRequest(slug.into())
}

#[test]
fn status_classes_decide() {
    let cases: &[(u16, ProviderError)] = &[
        (401, ProviderError::Unauthorized),
        (403, ProviderError::Unauthorized),
        (
            402,
            ProviderError::PaymentRequired(ProviderDetail::new(402, "")),
        ),
        (408, ProviderError::Timeout),
        (429, ProviderError::RateLimited(RetrySeconds(0))),
        (500, ProviderError::Server(ServerStatus(500))),
        (502, ProviderError::Server(ServerStatus(502))),
        (503, ProviderError::Server(ServerStatus(503))),
        (400, bad("http_400")),
        (404, bad("http_404")),
        (413, bad("http_413")),
        (422, bad("http_422")),
        (301, bad("http_301")),
    ];
    for (status, want) in cases {
        assert_eq!(&classify(*status, ""), want, "{status}");
        assert_eq!(&classify(*status, "not json"), want, "{status} with junk");
    }
}

#[test]
fn a_rate_limit_carries_the_retry_after_seconds() {
    let codec = OpenAiCodec::new(Flavor::LiteLlm);
    let with = head(429, BodyKind::Json, Some(12));
    assert_eq!(
        codec.classify(&with, b"{}"),
        ProviderError::RateLimited(RetrySeconds(12))
    );
    // An envelope that also says rate limit has no seconds; the head's win.
    let envelope = br#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#;
    assert_eq!(
        codec.classify(&with, envelope),
        ProviderError::RateLimited(RetrySeconds(12))
    );
    let a_400 = head(400, BodyKind::Json, Some(5));
    assert_eq!(
        codec.classify(&a_400, envelope),
        ProviderError::RateLimited(RetrySeconds(5))
    );
}

#[test]
fn llama_server_reports_a_loading_model_as_not_ready() {
    let body = r#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#;
    assert_eq!(classify(503, body), ProviderError::NotReady);
}

#[test]
fn a_context_overflow_carries_the_limit_from_the_envelope_or_the_message() {
    let llama = r#"{"error":{"code":400,"message":"request (9000 tokens) exceeds the available context size (4096 tokens)","type":"exceed_context_size_error","n_prompt_tokens":9000,"n_ctx":4096}}"#;
    assert_eq!(
        classify(400, llama),
        ProviderError::ContextOverflow {
            limit: Tokens(4096)
        }
    );
    let vllm = r#"{"error":{"message":"This model's maximum context length is 8192 tokens. However, you requested 9000 tokens (8000 in the messages, 1000 in the completion).","type":"BadRequestError","param":null,"code":400}}"#;
    assert_eq!(
        classify(400, vllm),
        ProviderError::ContextOverflow {
            limit: Tokens(8192)
        }
    );
    let openai = r#"{"error":{"message":"too long","type":"invalid_request_error","code":"context_length_exceeded"}}"#;
    assert!(matches!(
        classify(400, openai),
        ProviderError::ContextOverflow { .. }
    ));
}

#[test]
fn a_client_error_keeps_the_status_and_a_redacted_message() {
    let openrouter = r#"{"error":{"message":"This request requires more credits, or fewer max_tokens. You requested up to 65536 tokens, but can only afford 367.","code":402}}"#;
    let cases: &[(u16, &str, ProviderError)] = &[
        (
            402,
            openrouter,
            ProviderError::PaymentRequired(ProviderDetail::new(
                402,
                "This request requires more credits, or fewer max_tokens. You requested up to 65536 tokens, but can only afford 367.",
            )),
        ),
        (
            400,
            r#"{"error":{"message":"Provider returned error: response_format json_schema is not supported","type":"invalid_request_error","code":400}}"#,
            bad(
                "http_400 invalid_request_error: Provider returned error: response_format json_schema is not supported",
            ),
        ),
        (
            400,
            r#"{"error":"unknown field foo"}"#,
            bad("http_400 error: unknown field foo"),
        ),
        (
            404,
            r#"{"detail":"no such route"}"#,
            bad("http_404: no such route"),
        ),
        (
            401,
            r#"{"error":{"message":"No auth credentials found sk-or-v1-0123456789abcdef","code":401}}"#,
            ProviderError::AuthRejected(ProviderDetail::new(
                401,
                "No auth credentials found [redacted]",
            )),
        ),
        (
            403,
            r#"{"error":{"message":"key disabled"}}"#,
            ProviderError::AuthRejected(ProviderDetail::new(403, "key disabled")),
        ),
        (
            429,
            r#"{"error":{"message":"slow down"}}"#,
            ProviderError::RateLimited(RetrySeconds(0)),
        ),
    ];
    for (status, body, want) in cases {
        assert_eq!(&classify(*status, body), want, "{status} {body}");
    }
}

#[test]
fn a_provider_message_never_carries_a_credential_or_runs_long() {
    let body = format!(
        r#"{{"error":{{"message":"Authorization: Bearer abc123 api_key=hunter2 {}"}}}}"#,
        "long ".repeat(100)
    );
    let text = format!("{:?}", classify(400, &body));
    for secret in ["abc123", "hunter2"] {
        assert!(!text.contains(secret), "{text}");
    }
    assert!(text.len() < 400, "{text}");
    // A 5xx and a context overflow still carry no message.
    let leaky = r#"{"error":{"message":"SECRET prompt","type":"server_error"}}"#;
    assert!(!format!("{:?}", classify(500, leaky)).contains("SECRET"));
}

#[test]
fn an_envelope_cannot_turn_a_5xx_into_a_client_error() {
    let body = r#"{"error":{"message":"SECRET","type":"invalid_request_error"}}"#;
    assert_eq!(
        classify(500, body),
        ProviderError::Server(ServerStatus(500))
    );
}

#[test]
fn an_envelope_of_another_class_decides_a_client_error() {
    let body = r#"{"error":{"type":"authentication_error"}}"#;
    assert_eq!(classify(400, body), ProviderError::Unauthorized);
    assert_eq!(classify(401, "{}"), ProviderError::Unauthorized);
    assert_eq!(
        classify(403, r#"{"error":{"type":"overloaded"}}"#),
        ProviderError::Unauthorized
    );
}

#[test]
fn an_html_page_served_as_200_is_unreadable() {
    let codec = OpenAiCodec::new(Flavor::LiteLlm);
    let html = head(200, BodyKind::Html, None);
    assert_eq!(
        codec.classify(&html, b"<html>Sign in</html>"),
        ProviderError::Unreadable("an HTML page was served instead of a reply".into())
    );
}

#[test]
fn a_gateway_page_with_a_failing_status_is_by_status() {
    let codec = OpenAiCodec::new(Flavor::LiteLlm);
    assert_eq!(
        codec.classify(
            &head(502, BodyKind::Html, None),
            b"<html>Bad gateway</html>"
        ),
        ProviderError::Server(ServerStatus(502))
    );
}

#[test]
fn a_200_that_is_an_error_envelope_is_that_error() {
    let codec = OpenAiCodec::new(Flavor::Vllm);
    let ok = head(200, BodyKind::Json, None);
    assert_eq!(
        codec.classify(&ok, br#"{"error":{"type":"server_error","code":500}}"#),
        ProviderError::Server(ServerStatus(500))
    );
    assert_eq!(
        codec.classify(&ok, b"{}"),
        ProviderError::Unreadable("the reply is not an answer".into())
    );
}

#[test]
fn no_flavor_reads_errors_differently() {
    let body = br#"{"error":{"type":"rate_limit_error"}}"#;
    for flavor in [
        Flavor::LlamaServer,
        Flavor::Vllm,
        Flavor::LiteLlm,
        Flavor::OpenRouter,
    ] {
        assert_eq!(
            OpenAiCodec::new(flavor).classify(&head(429, BodyKind::Json, Some(2)), body),
            ProviderError::RateLimited(RetrySeconds(2))
        );
    }
}

#[test]
fn arbitrary_bytes_never_panic() {
    use proptest::prelude::*;
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(any::<u16>(), proptest::collection::vec(any::<u8>(), 0..300)),
            |(status, body)| {
                let _ = OpenAiCodec::new(Flavor::Vllm)
                    .classify(&head(status, BodyKind::Other, None), &body);
                Ok(())
            },
        )
        .unwrap();
}
