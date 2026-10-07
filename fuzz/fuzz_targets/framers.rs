//! The SSE and NDJSON framers: no panic, and the same lines whatever the chunking.
#![no_main]

use libfuzzer_sys::fuzz_target;
use model_http::{NdjsonDecoder, SseDecoder};
use stoker_fuzz::{pieces, split};

fuzz_target!(|data: &[u8]| {
    let (cuts, bytes) = split(data);
    let run_sse = |chunks: &[&[u8]]| {
        let mut d = SseDecoder::new();
        let mut events = Vec::new();
        for chunk in chunks {
            events.extend(d.feed(chunk).ok()?);
        }
        events.extend(d.finish());
        Some(events)
    };
    assert_eq!(run_sse(&pieces(bytes, &cuts)), run_sse(&[bytes]));
    let run_lines = |chunks: &[&[u8]]| {
        let mut d = NdjsonDecoder::new();
        let mut lines = Vec::new();
        for chunk in chunks {
            lines.extend(d.feed(chunk).ok()?);
        }
        lines.extend(d.finish());
        Some(lines)
    };
    assert_eq!(run_lines(&pieces(bytes, &cuts)), run_lines(&[bytes]));
});
