//! The pure decoders: SSE edge cases (the list ported from rig's framer tests) and NDJSON, each
//! with a split-at-every-offset check.

use model_http::{EventName, LineError, NdjsonDecoder, SseDecoder, SseError, SseEvent};

fn sse(chunks: &[&[u8]]) -> Result<Vec<SseEvent>, SseError> {
    let mut decoder = SseDecoder::new();
    let mut events = Vec::new();
    for chunk in chunks {
        events.extend(decoder.feed(chunk)?);
    }
    events.extend(decoder.finish());
    Ok(events)
}

fn event(name: Option<&str>, data: &str) -> SseEvent {
    SseEvent {
        event: name.map(|n| EventName(n.into())),
        data: data.into(),
    }
}

#[test]
fn every_line_ending_terminates_a_line() {
    for end in ["\n", "\r", "\r\n"] {
        let body = format!("data: a{end}{end}data: b{end}{end}");
        assert_eq!(
            sse(&[body.as_bytes()]).unwrap(),
            vec![event(None, "a"), event(None, "b")],
            "{end:?}"
        );
    }
    // Mixed endings in one stream.
    assert_eq!(
        sse(&[b"data: a\r\n\ndata: b\r\rdata: c\n\r\n"]).unwrap(),
        vec![event(None, "a"), event(None, "b"), event(None, "c")]
    );
}

#[test]
fn a_crlf_split_between_chunks_is_one_terminator() {
    let got = sse(&[b"data: a\r", b"\n\r", b"\ndata: b\r\n\r\n"]).unwrap();
    assert_eq!(got, vec![event(None, "a"), event(None, "b")]);
    // Were the CR taken as a line end on its own, the LF would add a blank line and a
    // second, empty event could appear after `data: b`.
    let got = sse(&[b"data: a\r\n\r", b"\n"]).unwrap();
    assert_eq!(got, vec![event(None, "a")]);
}

#[test]
fn a_trailing_cr_blank_line_dispatches_when_the_stream_ends() {
    assert_eq!(sse(&[b"data: a\r\r"]).unwrap(), vec![event(None, "a")]);
}

#[test]
fn a_bom_at_the_start_is_dropped_even_when_split() {
    assert_eq!(
        sse(&[b"\xEF\xBB\xBFdata: a\n\n"]).unwrap(),
        vec![event(None, "a")]
    );
    for cut in 1..3 {
        let body = b"\xEF\xBB\xBFdata: a\n\n";
        assert_eq!(
            sse(&[&body[..cut], &body[cut..]]).unwrap(),
            vec![event(None, "a")],
            "cut {cut}"
        );
    }
    assert_eq!(
        sse(&[b"\xEF", b"\xBB", b"\xBF", b"data: a\n\n"]).unwrap(),
        vec![event(None, "a")]
    );
    // A BOM later in the stream is data, not dropped.
    assert_eq!(
        sse(&[b"data: a\n\n\xEF\xBB\xBFdata: b\n\n"]).unwrap(),
        vec![event(None, "a")]
    );
}

#[test]
fn comments_are_ignored() {
    assert_eq!(
        sse(&[b": keep-alive\n\ndata: a\n: mid\n\n"]).unwrap(),
        vec![event(None, "a")]
    );
    assert_eq!(sse(&[b":\n:data: x\n\n"]).unwrap(), vec![]);
}

#[test]
fn data_lines_join_with_newlines_and_one_space_is_dropped() {
    const CASES: &[(&str, &str, &str)] = &[
        ("two lines", "data: a\ndata: b\n\n", "a\nb"),
        ("no space", "data:a\n\n", "a"),
        ("one space dropped", "data:  a\n\n", " a"),
        ("empty data", "data:\n\n", ""),
        ("empty value with space", "data: \n\n", ""),
        ("no colon", "data\n\n", ""),
        ("empty middle line", "data: a\ndata\ndata: b\n\n", "a\n\nb"),
        ("colon inside", "data: a:b\n\n", "a:b"),
    ];
    for (label, body, data) in CASES {
        assert_eq!(
            sse(&[body.as_bytes()]).unwrap(),
            vec![event(None, data)],
            "{label}"
        );
    }
}

#[test]
fn event_names_apply_to_one_event_only() {
    let got =
        sse(&[b"event: ping\ndata: 1\n\ndata: 2\n\nevent: x\nevent: y\ndata: 3\n\n"]).unwrap();
    assert_eq!(
        got,
        vec![
            event(Some("ping"), "1"),
            event(None, "2"),
            event(Some("y"), "3")
        ]
    );
    assert_eq!(
        sse(&[b"event:\ndata: a\n\n"]).unwrap(),
        vec![event(None, "a")]
    );
}

#[test]
fn a_blank_line_without_data_dispatches_nothing_and_resets_the_type() {
    let got = sse(&[b"event: ping\n\ndata: a\n\n\n\n"]).unwrap();
    assert_eq!(got, vec![event(None, "a")]);
}

#[test]
fn id_and_retry_are_not_kept_and_unknown_fields_are_ignored() {
    let got = sse(&[b"id: 7\nretry: 100\nid: a\0b\nfoo: bar\ndata: a\n\ndata: b\n\n"]).unwrap();
    assert_eq!(got, vec![event(None, "a"), event(None, "b")]);
}

#[test]
fn a_truncated_trailing_event_is_never_delivered() {
    assert_eq!(
        sse(&[b"data: a\n\ndata: b\n"]).unwrap(),
        vec![event(None, "a")]
    );
    assert_eq!(
        sse(&[b"data: a\n\ndata: b"]).unwrap(),
        vec![event(None, "a")]
    );
    assert_eq!(sse(&[b"event: x\ndata: a"]).unwrap(), vec![]);
}

#[test]
fn a_multibyte_code_point_split_across_chunks_is_not_an_error() {
    let body = "data: h\u{e9}llo \u{1f600}\n\n".as_bytes();
    for cut in 0..=body.len() {
        let got = sse(&[&body[..cut], &body[cut..]]).unwrap();
        assert_eq!(got, vec![event(None, "h\u{e9}llo \u{1f600}")], "cut {cut}");
    }
}

#[test]
fn a_complete_line_that_is_not_utf8_is_an_error() {
    assert_eq!(sse(&[b"data: \xFF\xFE\n\n"]), Err(SseError::NotUtf8));
    // An incomplete one is not decoded yet, and a dropped trailing event never errors.
    assert_eq!(
        sse(&[b"data: a\n\ndata: \xFF"]).unwrap(),
        vec![event(None, "a")]
    );
}

#[test]
fn lines_and_events_have_caps() {
    let long = vec![b'a'; (1 << 20) + 8];
    assert_eq!(SseDecoder::new().feed(&long), Err(SseError::LineTooLong));
    let mut with_end = long.clone();
    with_end.push(b'\n');
    assert_eq!(
        SseDecoder::new().feed(&with_end),
        Err(SseError::LineTooLong)
    );
    // Many short lines, one event: the event cap, not the line cap, stops it.
    let mut decoder = SseDecoder::new();
    let line = format!("data: {}\n", "x".repeat(60_000));
    let outcome = (0..200).try_for_each(|_| decoder.feed(line.as_bytes()).map(drop));
    assert_eq!(outcome, Err(SseError::LineTooLong));
    // A long event under the cap is fine.
    let mut ok = SseDecoder::new();
    let fits = format!("data: {}\n\n", "x".repeat(900_000));
    assert_eq!(ok.feed(fits.as_bytes()).unwrap().len(), 1);
}

const STREAM: &[u8] = b"\xEF\xBB\xBF: hi\r\nevent: a\r\ndata: 1\r\ndata: 2\r\n\r\ndata: h\xC3\xA9\xF0\x9F\x98\x80\n\n\revent: b\rdata: x\r\r:c\n\ndata: tail\n";

#[test]
fn splitting_a_body_at_every_offset_yields_the_same_events() {
    let whole = sse(&[STREAM]).unwrap();
    assert_eq!(
        whole,
        vec![
            event(Some("a"), "1\n2"),
            event(None, "h\u{e9}\u{1f600}"),
            event(Some("b"), "x")
        ]
    );
    for cut in 0..=STREAM.len() {
        assert_eq!(
            sse(&[&STREAM[..cut], &STREAM[cut..]]).unwrap(),
            whole,
            "cut {cut}"
        );
    }
    let bytes: Vec<&[u8]> = STREAM.chunks(1).collect();
    assert_eq!(sse(&bytes).unwrap(), whole, "one byte at a time");
}

fn ndjson(chunks: &[&[u8]]) -> Result<(Vec<String>, Option<String>), LineError> {
    let mut decoder = NdjsonDecoder::new();
    let mut lines = Vec::new();
    for chunk in chunks {
        lines.extend(decoder.feed(chunk)?);
    }
    Ok((lines, decoder.finish()))
}

#[test]
fn ndjson_lines() {
    const CASES: &[(&str, &str, &[&str], Option<&str>)] = &[
        (
            "two lines",
            "{\"a\":1}\n{\"b\":2}\n",
            &["{\"a\":1}", "{\"b\":2}"],
            None,
        ),
        (
            "blank lines are skipped",
            "\n\n{\"a\":1}\n   \n\r\n",
            &["{\"a\":1}"],
            None,
        ),
        (
            "crlf",
            "{\"a\":1}\r\n{\"b\":2}\r\n",
            &["{\"a\":1}", "{\"b\":2}"],
            None,
        ),
        (
            "unterminated last line",
            "{\"a\":1}\n{\"b\":",
            &["{\"a\":1}"],
            Some("{\"b\":"),
        ),
        ("only an unterminated line", "{}", &[], Some("{}")),
        ("a blank tail is nothing", "{}\n  ", &["{}"], None),
        ("empty", "", &[], None),
    ];
    for (label, body, lines, tail) in CASES {
        let want: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
        assert_eq!(
            ndjson(&[body.as_bytes()]).unwrap(),
            (want, tail.map(str::to_owned)),
            "{label}"
        );
    }
}

#[test]
fn ndjson_is_split_safe_and_strict() {
    let body = "{\"t\":\"h\u{e9}\u{1f600}\"}\r\n\n{\"b\":2}\n{\"c\"".as_bytes();
    let whole = ndjson(&[body]).unwrap();
    assert_eq!(whole.0.len(), 2);
    assert_eq!(whole.1.as_deref(), Some("{\"c\""));
    for cut in 0..=body.len() {
        assert_eq!(
            ndjson(&[&body[..cut], &body[cut..]]).unwrap(),
            whole,
            "cut {cut}"
        );
    }
    assert_eq!(ndjson(&[b"\xFF\n"]), Err(LineError::NotUtf8));
    assert_eq!(
        NdjsonDecoder::new().feed(&vec![b'a'; (4 << 20) + 1]),
        Err(LineError::LineTooLong)
    );
}
