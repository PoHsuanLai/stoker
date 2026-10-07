//! The SSE and NDJSON framers over arbitrary bytes and arbitrary splits: no panic, a typed
//! result, the same lines however the bytes arrive, and linear time on adversarial shapes.

use model_http::{LineError, NdjsonDecoder, SseDecoder, SseError, SseEvent};
use proptest::prelude::*;

fn sse(chunks: &[&[u8]]) -> Result<Vec<SseEvent>, SseError> {
    let mut decoder = SseDecoder::new();
    let mut events = Vec::new();
    for chunk in chunks {
        events.extend(decoder.feed(chunk)?);
    }
    events.extend(decoder.finish());
    Ok(events)
}

fn ndjson(chunks: &[&[u8]]) -> Result<Vec<String>, LineError> {
    let mut decoder = NdjsonDecoder::new();
    let mut lines = Vec::new();
    for chunk in chunks {
        lines.extend(decoder.feed(chunk)?);
    }
    lines.extend(decoder.finish());
    Ok(lines)
}

fn cut<'a>(bytes: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
    points.extend([0, bytes.len()]);
    points.sort_unstable();
    points.windows(2).map(|w| &bytes[w[0]..w[1]]).collect()
}

fn shaped() -> impl Strategy<Value = Vec<u8>> {
    let token = prop_oneof![
        Just("data: ".to_owned()),
        Just("event: x".to_owned()),
        Just(": c".to_owned()),
        Just("\n".to_owned()),
        Just("\r".to_owned()),
        Just("\r\n".to_owned()),
        Just("\u{feff}".to_owned()),
        Just("é世".to_owned()),
        "[ -~]{0,5}",
    ];
    proptest::collection::vec(token, 0..30).prop_map(|t| t.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn sse_over_arbitrary_bytes_is_typed_and_split_invariant(
        bytes in prop_oneof![proptest::collection::vec(any::<u8>(), 0..600), shaped()],
        cuts in proptest::collection::vec(any::<usize>(), 0..8),
    ) {
        let split = sse(&cut(&bytes, &cuts));
        let whole = sse(&[&bytes]);
        match (&split, &whole) {
            (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
            (Err(_), Err(_)) => {}
            other => prop_assert!(false, "verdicts differ: {other:?}"),
        }
    }

    #[test]
    fn ndjson_over_arbitrary_bytes_is_typed_and_split_invariant(
        bytes in prop_oneof![proptest::collection::vec(any::<u8>(), 0..600), shaped()],
        cuts in proptest::collection::vec(any::<usize>(), 0..8),
    ) {
        let split = ndjson(&cut(&bytes, &cuts));
        let whole = ndjson(&[&bytes]);
        match (&split, &whole) {
            (Ok(a), Ok(b)) => prop_assert_eq!(a, b),
            (Err(_), Err(_)) => {}
            other => prop_assert!(false, "verdicts differ: {other:?}"),
        }
    }
}

#[test]
fn a_chunk_of_many_short_lines_is_linear_for_both_framers() {
    // Draining the consumed prefix once per line made these quadratic.
    let newlines = vec![b'\n'; 4 << 20];
    assert_eq!(sse(&[&newlines]), Ok(Vec::new()));
    assert_eq!(ndjson(&[&newlines]), Ok(Vec::new()));
    let crs = vec![b'\r'; 1 << 20];
    assert_eq!(sse(&[&crs]), Ok(Vec::new()));
    let events = b"data: a\n\n".repeat(200_000);
    assert_eq!(sse(&[&events]).unwrap().len(), 200_000);
    let lines = b"{}\n".repeat(400_000);
    assert_eq!(ndjson(&[&lines]).unwrap().len(), 400_000);
}

#[test]
fn a_long_line_fed_a_byte_at_a_time_is_scanned_once() {
    // Rescanning the pending line on every feed made this 4e10 byte comparisons per framer.
    let line = [b"data: ".as_slice(), &vec![b'a'; 300_000]].concat();
    let bytes: Vec<&[u8]> = line.chunks(1).collect();
    assert_eq!(sse(&bytes), Ok(Vec::new()));
    let text = vec![b'a'; 300_000];
    let bytes: Vec<&[u8]> = text.chunks(1).collect();
    assert_eq!(ndjson(&bytes), Ok(vec!["a".repeat(300_000)]));
}

#[test]
fn an_unterminated_line_over_the_cap_is_an_error_not_growth() {
    let line = vec![b'a'; (4 << 20) + 1];
    assert_eq!(ndjson(&[&line]), Err(LineError::LineTooLong));
    let line = vec![b'a'; (1 << 20) + 1];
    assert_eq!(sse(&[&line]), Err(SseError::LineTooLong));
}
