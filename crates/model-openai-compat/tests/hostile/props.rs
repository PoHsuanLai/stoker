//! Arbitrary bytes and arbitrary splits: no panic, a typed result, the same answer however the
//! bytes arrive.

use model_http::{BodyKind, HttpStatus, ResponseHead};
use model_openai_compat::{Flavor, KOKORO_FORMAT, OpenAiCodec, PcmDecoder, decode_transcription};
use model_provider::ModelName;
use model_wire::{ChatCodec, EmbedCodec, ErrorWire};
use proptest::prelude::*;

use crate::pipe::{Reply, assert_sound, call, calls, chunk, run_cut, scenario, sse};
use serde_json::json;

fn cuts() -> impl Strategy<Value = Vec<usize>> {
    proptest::collection::vec(any::<usize>(), 0..8)
}

/// What must not depend on chunking: the verdict and, for a reply that ends cleanly, its events.
/// Which error a broken stream reports can depend on it (a chunk is framed whole before its frames
/// are decoded), so two refusals agree whatever their kinds.
fn same(a: &Reply, b: &Reply) -> bool {
    match (&a.end, &b.end) {
        (Err(_), Err(_)) => true,
        _ => a == b,
    }
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let token = prop_oneof![
        Just("data: ".to_owned()),
        Just("data:".to_owned()),
        Just("event: ping".to_owned()),
        Just(": comment".to_owned()),
        Just("\n".to_owned()),
        Just("\r\n".to_owned()),
        Just("\r".to_owned()),
        Just("[DONE]".to_owned()),
        Just("{\"choices\":[{\"index\":0,\"delta\":{".to_owned()),
        Just("\"content\":\"é世\"".to_owned()),
        Just("\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]".to_owned()),
        Just("},\"finish_reason\":\"stop\"}]}".to_owned()),
        Just("\u{feff}".to_owned()),
        "[ -~]{0,6}",
    ];
    proptest::collection::vec(token, 0..24).prop_map(|t| t.concat().into_bytes())
}

fn pool() -> Vec<String> {
    let mut frames = scenario();
    frames.extend([
        chunk(
            json!({"content": "<tool_call>{\"name\":\"f\"}</tool_call>"}),
            None,
        ),
        chunk(
            calls(vec![call(0, Some("z"), Some("f"), Some("[1]"))]),
            None,
        ),
        chunk(
            calls(vec![call(0, Some("y"), Some("g"), Some("{\"a\":"))]),
            None,
        ),
        chunk(calls(vec![call(3, None, Some("h"), None)]), None),
        chunk(json!({}), Some("banana")),
        chunk(json!({}), Some("length")),
        json!({"choices": []}).to_string(),
        json!({"choices": [], "usage": {"prompt_tokens": u64::MAX}}).to_string(),
        json!({"error": {"type": "server_error"}}).to_string(),
        "[DONE]".to_owned(),
        "{\"choices\":[{\"delta\":{\"content\"".to_owned(),
    ]);
    frames
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn arbitrary_bytes_end_in_a_typed_result(bytes in proptest::collection::vec(any::<u8>(), 0..1500), cuts in cuts()) {
        let split = run_cut(&bytes, &cuts);
        assert_sound(&split);
        prop_assert!(same(&split, &run_cut(&bytes, &[])));
    }

    #[test]
    fn sse_shaped_bytes_end_in_a_typed_result_whatever_the_split(bytes in soup(), cuts in cuts()) {
        let split = run_cut(&bytes, &cuts);
        assert_sound(&split);
        prop_assert!(same(&split, &run_cut(&bytes, &[])));
    }

    #[test]
    fn shuffled_duplicated_and_mutated_frames_stay_sound(
        picks in proptest::collection::vec(0usize..32, 0..14),
        with_done in any::<bool>(),
        cuts in cuts(),
    ) {
        let pool = pool();
        let frames: Vec<String> = picks.iter().map(|p| pool[p % pool.len()].clone()).collect();
        let mut body = sse(&frames);
        if !with_done {
            body.truncate(body.len() - "data: [DONE]\n\n".len());
        }
        let split = run_cut(&body, &cuts);
        assert_sound(&split);
        prop_assert!(same(&split, &run_cut(&body, &[])));
    }

    #[test]
    fn a_valid_reply_cut_short_or_followed_by_noise_stays_sound(
        keep in any::<usize>(),
        noise in proptest::collection::vec(any::<u8>(), 0..64),
        cuts in cuts(),
    ) {
        let mut body = sse(&scenario());
        body.truncate(keep % (body.len() + 1));
        body.extend(noise);
        let split = run_cut(&body, &cuts);
        assert_sound(&split);
        prop_assert!(same(&split, &run_cut(&body, &[])));
    }

    #[test]
    fn whole_bodies_never_panic(body in proptest::collection::vec(any::<u8>(), 0..600)) {
        let codec = OpenAiCodec::new(Flavor::Vllm);
        let _ = codec.parse_models(&body);
        let _ = codec.decode_embed(ModelName("m".into()), &body);
        let _ = decode_transcription(&body);
        for status in [200, 400, 429, 500] {
            for kind in [BodyKind::Json, BodyKind::Html] {
                let head = ResponseHead { status: HttpStatus(status), body: kind, retry_after: None, request_id: None };
                let _ = codec.classify(&head, &body);
            }
        }
    }

    #[test]
    fn json_shaped_bodies_never_panic(text in "\\{\"(data|id|embedding|index|usage|error|text|default_generation_settings|n_ctx|model_alias)\":[-0-9a-z\\[\\]{}\":,. ]{0,40}") {
        let codec = OpenAiCodec::new(Flavor::LlamaServer);
        let body = text.as_bytes();
        let _ = codec.parse_models(body);
        let _ = codec.decode_embed(ModelName("m".into()), body);
        let _ = decode_transcription(body);
    }

    #[test]
    fn pcm_arrives_whole_samples_whatever_the_split(bytes in proptest::collection::vec(any::<u8>(), 0..300), cuts in cuts()) {
        let width = KOKORO_FORMAT.pcm.width() as usize;
        let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
        points.extend([0, bytes.len()]);
        points.sort_unstable();
        let mut decoder = PcmDecoder::new(KOKORO_FORMAT);
        let mut got = 0usize;
        for w in points.windows(2) {
            if let Some(chunk) = decoder.feed(&bytes[w[0]..w[1]]) {
                prop_assert_eq!(chunk.pcm.as_slice().len() % width, 0);
                got += chunk.pcm.as_slice().len();
            }
        }
        prop_assert_eq!(got, bytes.len() - bytes.len() % width);
    }
}
