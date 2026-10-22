//! `read_calls` is total: the first byte picks the format, the rest is the answer, a NUL and the
//! thought. Any bytes, no panic, and never more calls than the limits allow.
#![no_main]

use cua_action::CallFormat;
use cua_parse::{ParamKinds, ParseLimits, read_calls};
use libfuzzer_sys::fuzz_target;

const SCHEMA: &str = r#"{"properties":{"action":{"type":"string"},"text":{"type":"string"},"coordinate":{"type":"array"},"pixels":{"type":"integer"}}}"#;

fuzz_target!(|data: &[u8]| {
    let Some((head, rest)) = data.split_first() else {
        return;
    };
    let format = if head & 1 == 0 {
        CallFormat::Qwen3CoderXml
    } else {
        CallFormat::HermesJson
    };
    let text = String::from_utf8_lossy(rest);
    let (answer, thought) = text.split_once('\0').unwrap_or((&text, ""));
    let limits = ParseLimits::default();
    if let Ok(calls) = read_calls(
        format,
        answer,
        thought,
        &ParamKinds::from_schema(SCHEMA),
        limits,
    ) {
        assert!(calls.len() <= usize::from(limits.max_actions.0));
    }
});
