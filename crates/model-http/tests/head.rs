//! The pure readings of a response head: what a `Content-Type` says and what `Retry-After` means.

use model_http::{BodyKind, WaitSeconds};

#[test]
fn a_content_type_names_the_kind_of_body() {
    let cases = [
        ("text/event-stream", BodyKind::EventStream),
        ("text/event-stream; charset=utf-8", BodyKind::EventStream),
        ("TEXT/Event-Stream", BodyKind::EventStream),
        ("application/json", BodyKind::Json),
        ("application/json; charset=utf-8", BodyKind::Json),
        ("application/problem+json", BodyKind::Json),
        ("application/x-ndjson", BodyKind::NdJson),
        ("application/jsonl", BodyKind::NdJson),
        ("text/html", BodyKind::Html),
        ("text/html; charset=UTF-8", BodyKind::Html),
        ("application/xhtml+xml", BodyKind::Html),
        ("text/plain", BodyKind::Other),
        ("application/octet-stream", BodyKind::Other),
        ("", BodyKind::Other),
        ("garbage", BodyKind::Other),
    ];
    for (content_type, want) in cases {
        assert_eq!(BodyKind::of(content_type), want, "{content_type:?}");
    }
}

#[test]
fn retry_after_is_read_in_its_seconds_form_only() {
    let cases = [
        ("7", Some(7)),
        (" 30 ", Some(30)),
        ("0", Some(0)),
        ("4294967295", Some(u32::MAX)),
        ("4294967296", None),
        ("-1", None),
        ("1.5", None),
        ("", None),
        ("Wed, 21 Oct 2026 07:28:00 GMT", None),
        ("+5", None),
        ("5s", None),
    ];
    for (value, want) in cases {
        assert_eq!(
            WaitSeconds::from_header(value),
            want.map(WaitSeconds),
            "{value:?}"
        );
    }
}

#[test]
fn any_text_is_a_kind_and_never_a_panic() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&".{0,60}", |text| {
            let _ = BodyKind::of(&text);
            let _ = WaitSeconds::from_header(&text);
            Ok(())
        })
        .unwrap();
}
