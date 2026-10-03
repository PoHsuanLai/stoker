//! The driver over a scripted transport: the head decides what the body is, the framer the
//! exchange named cuts frames, the decoder's events reach the sink, every failure is a
//! `ProviderError` that carries no body.

mod support;

use model_http::{BodyKind, Framing, HttpError, HttpStatus};
use model_provider::{
    Dims, Embedder, Knob, ModelInfo, ModelName, Provider, ProviderError, RetrySeconds, StopReason,
    Tokens, TurnEnd, TurnEvent, TurnUsage,
};
use model_wire::Driver;
use support::*;

fn sse(frames: &[&str]) -> String {
    frames.iter().map(|f| format!("data: {f}\n\n")).collect()
}

fn driver(framing: Framing, script: Vec<Canned>) -> Driver<LineCodec, Scripted> {
    Driver::new(LineCodec { framing }, Scripted::new(script))
}

fn turn(driver: &Driver<LineCodec, Scripted>, sink: &mut Keep) -> Result<TurnEnd, ProviderError> {
    block_on(driver.turn(&request("m"), sink))
}

fn end(usage: TurnUsage) -> TurnEnd {
    TurnEnd {
        stop: StopReason::EndTurn,
        usage,
        served: ModelName("m".into()),
    }
}

#[test]
fn a_stream_reaches_the_sink_and_ends_with_the_decoders_end() {
    let body = sse(&["text:he", "text:llo", "usage:3,2", "done"]);
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::EventStream), &[&body])],
    );
    let mut sink = Keep::default();
    let usage = TurnUsage {
        input: Tokens(3),
        output: Tokens(2),
        ..TurnUsage::default()
    };
    assert_eq!(turn(&d, &mut sink), Ok(end(usage)));
    assert_eq!(
        sink.events,
        vec![
            TurnEvent::TextDelta("he".into()),
            TurnEvent::TextDelta("llo".into()),
            TurnEvent::Usage(usage),
        ]
    );
    let asked = d.transport().asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].path.0, "/chat");
}

#[test]
fn every_byte_split_of_the_body_gives_the_same_turn() {
    let body = sse(&["text:caf\u{e9}", "text:!", "done"]);
    let bytes = body.as_bytes();
    for step in 1..=bytes.len() {
        let chunks: Vec<Vec<u8>> = bytes.chunks(step).map(<[u8]>::to_vec).collect();
        let canned = Canned {
            head: Some(head(200, BodyKind::EventStream)),
            chunks,
            result: Ok(HttpStatus(200)),
        };
        let d = driver(Framing::Sse, vec![canned]);
        let mut sink = Keep::default();
        assert_eq!(turn(&d, &mut sink), Ok(end(TurnUsage::default())), "{step}");
        assert_eq!(
            sink.events,
            vec![
                TurnEvent::TextDelta("caf\u{e9}".into()),
                TurnEvent::TextDelta("!".into())
            ],
            "{step}"
        );
    }
}

#[test]
fn ndjson_lines_are_frames_and_an_unterminated_last_line_is_read() {
    let d = driver(
        Framing::Ndjson,
        vec![Canned::ok(
            head(200, BodyKind::NdJson),
            &["text:a\r\n\r\ntext:b\n", "done"],
        )],
    );
    let mut sink = Keep::default();
    assert_eq!(turn(&d, &mut sink), Ok(end(TurnUsage::default())));
    assert_eq!(
        sink.events,
        vec![
            TurnEvent::TextDelta("a".into()),
            TurnEvent::TextDelta("b".into())
        ]
    );
}

#[test]
fn a_whole_body_is_one_frame() {
    let d = driver(
        Framing::Whole,
        vec![Canned::ok(head(200, BodyKind::Json), &["text:one ", "two"])],
    );
    let mut sink = Keep::default();
    // The decoder reads `text:one two` as one frame, then never sees `done`.
    assert_eq!(
        turn(&d, &mut sink),
        Err(ProviderError::Unreadable(
            "the stream ended before a finish reason".into()
        ))
    );
    assert_eq!(sink.events, vec![TurnEvent::TextDelta("one two".into())]);
}

#[test]
fn a_failing_status_is_classified_from_its_head_and_never_decoded() {
    let d = driver(
        Framing::Sse,
        vec![Canned::rejected(
            head(401, BodyKind::Json),
            "{\"error\":\"x\"}",
        )],
    );
    let mut sink = Keep::default();
    assert_eq!(turn(&d, &mut sink), Err(ProviderError::Unauthorized));
    assert!(sink.events.is_empty());
}

#[test]
fn an_error_body_is_buffered_up_to_64_kib() {
    let big = "x".repeat(200_000);
    let canned = Canned {
        head: Some(head(400, BodyKind::Json)),
        chunks: vec![big.as_bytes().chunks(10_000).next().unwrap().to_vec(); 20],
        result: Err(HttpError::Rejected),
    };
    let d = driver(Framing::Sse, vec![canned]);
    let mut sink = Keep::default();
    assert_eq!(
        turn(&d, &mut sink),
        Err(ProviderError::BadRequest("400 65536 bytes".into()))
    );
    assert!(
        d.transport().was_stopped(),
        "the rest of the body is not read"
    );
}

#[test]
fn an_html_page_served_as_200_is_classified_not_decoded() {
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(
            head(200, BodyKind::Html),
            &["<html>", "login</html>"],
        )],
    );
    let mut sink = Keep::default();
    assert_eq!(
        turn(&d, &mut sink),
        Err(ProviderError::Unreadable("html 18 bytes".into()))
    );
    assert!(sink.events.is_empty());
}

#[test]
fn a_rate_limit_carries_the_retry_after_of_the_head() {
    let h = with_retry_after(head(429, BodyKind::Json), 7);
    let d = driver(Framing::Sse, vec![Canned::rejected(h, "{}")]);
    assert_eq!(
        turn(&d, &mut Keep::default()),
        Err(ProviderError::RateLimited(RetrySeconds(7)))
    );
}

#[test]
fn an_error_envelope_inside_a_200_is_the_decoders_fault_with_the_heads_retry_after() {
    let h = with_retry_after(head(200, BodyKind::EventStream), 11);
    let body = sse(&["text:a", "fault:429", "text:never"]);
    let d = driver(Framing::Sse, vec![Canned::ok(h, &[&body])]);
    let mut sink = Keep::default();
    assert_eq!(
        turn(&d, &mut sink),
        Err(ProviderError::RateLimited(RetrySeconds(11)))
    );
    assert_eq!(sink.events, vec![TurnEvent::TextDelta("a".into())]);
    assert!(
        d.transport().was_stopped(),
        "the connection closes at the fault"
    );
}

#[test]
fn an_envelope_fault_without_a_retry_after_keeps_its_zero() {
    let body = sse(&["fault:429"]);
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::EventStream), &[&body])],
    );
    assert_eq!(
        turn(&d, &mut Keep::default()),
        Err(ProviderError::RateLimited(RetrySeconds(0)))
    );
}

#[test]
fn an_unreadable_frame_without_a_fault_is_a_generic_unreadable_error() {
    let body = sse(&["bad"]);
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::EventStream), &[&body])],
    );
    assert_eq!(
        turn(&d, &mut Keep::default()),
        Err(ProviderError::Unreadable("a frame is not a chunk".into()))
    );
}

#[test]
fn a_sink_that_stops_ends_the_turn_early_with_end_turn_and_the_usage_so_far() {
    let body = sse(&["usage:5,1", "text:a", "text:b", "done"]);
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::EventStream), &[&body])],
    );
    let mut sink = Keep {
        stop_after: Some(2),
        ..Keep::default()
    };
    let usage = TurnUsage {
        input: Tokens(5),
        output: Tokens(1),
        ..TurnUsage::default()
    };
    assert_eq!(turn(&d, &mut sink), Ok(end(usage)));
    assert_eq!(sink.events.len(), 2, "nothing after the stop is delivered");
    assert!(d.transport().was_stopped());
}

#[test]
fn transport_failures_map_to_provider_errors() {
    let cases = [
        (HttpError::Connect, ProviderError::Unreachable),
        (HttpError::Tls, ProviderError::Unreachable),
        (HttpError::Timeout, ProviderError::Timeout),
        (HttpError::Broken, ProviderError::Unreachable),
        (
            HttpError::Status(HttpStatus(503)),
            ProviderError::Server(model_provider::ServerStatus(503)),
        ),
        (
            HttpError::Status(HttpStatus(404)),
            ProviderError::BadRequest("http_404".into()),
        ),
        (
            HttpError::ReplayMiss,
            ProviderError::BadRequest("no recorded exchange".into()),
        ),
    ];
    for (error, want) in cases {
        let d = driver(Framing::Sse, vec![Canned::fails(error)]);
        assert_eq!(turn(&d, &mut Keep::default()), Err(want), "{error:?}");
    }
}

#[test]
fn a_connection_that_breaks_mid_stream_is_unreachable_unless_the_turn_was_complete() {
    let half = sse(&["text:a"]);
    let cut = Canned {
        head: Some(head(200, BodyKind::EventStream)),
        chunks: vec![half.into_bytes()],
        result: Err(HttpError::Broken),
    };
    let mut sink = Keep::default();
    assert_eq!(
        turn(&driver(Framing::Sse, vec![cut]), &mut sink),
        Err(ProviderError::Unreachable)
    );
    assert_eq!(sink.events.len(), 1, "what arrived was delivered");

    let whole = sse(&["text:a", "done"]);
    let late = Canned {
        head: Some(head(200, BodyKind::EventStream)),
        chunks: vec![whole.into_bytes()],
        result: Err(HttpError::Broken),
    };
    assert_eq!(
        turn(&driver(Framing::Sse, vec![late]), &mut Keep::default()),
        Ok(end(TurnUsage::default())),
        "a reset after the finish does not undo the turn"
    );
}

#[test]
fn a_stream_cut_before_its_finish_is_unreadable() {
    let body = sse(&["text:a"]);
    let d = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::EventStream), &[&body])],
    );
    assert_eq!(
        turn(&d, &mut Keep::default()),
        Err(ProviderError::Unreadable(
            "the stream ended before a finish reason".into()
        ))
    );
}

#[test]
fn an_unencodable_request_never_reaches_the_transport() {
    let d = driver(Framing::Sse, vec![]);
    let result = block_on(d.turn(&request("unsupported"), &mut Keep::default()));
    assert_eq!(
        result,
        Err(ProviderError::BadRequest(
            "unsupported shape for this wire".into()
        ))
    );
    assert!(d.transport().asked().is_empty());
}

#[test]
fn describe_parses_the_models_and_classifies_a_failure() {
    let ok = Canned::ok(head(200, BodyKind::Json), &["a 4096\n", "b 8192"]);
    let d = driver(Framing::Sse, vec![ok]);
    assert_eq!(
        block_on(d.describe()),
        Ok(vec![
            ModelInfo {
                name: ModelName("a".into()),
                loaded_context: Tokens(4096),
                trained_context: Tokens(4096)
            },
            ModelInfo {
                name: ModelName("b".into()),
                loaded_context: Tokens(8192),
                trained_context: Tokens(8192)
            },
        ])
    );
    assert_eq!(d.transport().asked()[0].path.0, "/models");

    let bad = driver(
        Framing::Sse,
        vec![Canned::rejected(head(401, BodyKind::Json), "{}")],
    );
    assert_eq!(block_on(bad.describe()), Err(ProviderError::Unauthorized));

    let junk = driver(
        Framing::Sse,
        vec![Canned::ok(head(200, BodyKind::Json), &["nonsense"])],
    );
    assert_eq!(
        block_on(junk.describe()),
        Err(ProviderError::Unreadable("a frame is not a chunk".into()))
    );

    let down = driver(Framing::Sse, vec![Canned::fails(HttpError::Connect)]);
    assert_eq!(block_on(down.describe()), Err(ProviderError::Unreachable));
}

fn embedder(script: Vec<Canned>) -> Driver<VecCodec, Scripted> {
    Driver::new(VecCodec, Scripted::new(script))
}

#[test]
fn embed_returns_one_vector_per_input() {
    let d = embedder(vec![Canned::ok(head(200, BodyKind::Json), &["1,2;3,4"])]);
    let end = block_on(d.embed(&embed_turn(&["a", "b"], Knob::Set(Dims(2))))).unwrap();
    assert_eq!(end.vectors.len(), 2);
    assert_eq!(end.vectors[1].0, vec![3.0, 4.0]);
    assert_eq!(end.served, ModelName("e".into()));
}

#[test]
fn embed_refuses_a_wrong_count_or_width_before_a_vector_reaches_a_store() {
    let count = embedder(vec![Canned::ok(head(200, BodyKind::Json), &["1,2"])]);
    assert_eq!(
        block_on(count.embed(&embed_turn(&["a", "b"], Knob::Set(Dims(2))))),
        Err(ProviderError::Unreadable(
            "asked for 2 vectors, got 1".into()
        ))
    );
    let width = embedder(vec![Canned::ok(
        head(200, BodyKind::Json),
        &["1,2,3;4,5,6"],
    )]);
    assert_eq!(
        block_on(width.embed(&embed_turn(&["a", "b"], Knob::Set(Dims(2))))),
        Err(ProviderError::Unreadable(
            "expected vectors of width 2, got 3".into()
        ))
    );
    let ragged = embedder(vec![Canned::ok(head(200, BodyKind::Json), &["1,2;3"])]);
    assert_eq!(
        block_on(ragged.embed(&embed_turn(&["a", "b"], Knob::Off))),
        Err(ProviderError::Unreadable(
            "expected vectors of width 2, got 1".into()
        )),
        "with no width asked for, the first vector sets it"
    );
}

#[test]
fn embed_with_no_inputs_asks_nothing() {
    let d = embedder(vec![]);
    let end = block_on(d.embed(&embed_turn(&[], Knob::Off))).unwrap();
    assert!(end.vectors.is_empty());
    assert!(d.transport().asked().is_empty());
}

#[test]
fn embed_classifies_a_failing_status() {
    let d = embedder(vec![Canned::rejected(head(413, BodyKind::Json), "{}")]);
    assert_eq!(
        block_on(d.embed(&embed_turn(&["a"], Knob::Off))),
        Err(ProviderError::BadRequest("413".into()))
    );
}
