//! First-token log-probabilities: the request fields and the decoder's record, over frames written
//! by hand in the shape vLLM and llama-server send (`choices[0].logprobs.content[].top_logprobs`).

use model_openai_compat::{Flavor, StreamDecoder, encode_request};
use model_provider::{
    ChoiceScores, Count, FirstTokenLogprobs, Logprob, ModelName, OutputShape, TokenLogprob,
};
use model_wire::ChatDecoder;
use serde_json::{Value, json};

use crate::support;

fn body(flavor: Flavor, scores: ChoiceScores) -> Value {
    let mut request = support::base();
    // Gateways cannot enforce a choice, so theirs is a free reply.
    request.output = match flavor {
        Flavor::Vllm | Flavor::LlamaServer => {
            OutputShape::Choice(vec!["pass".into(), "flag".into()])
        }
        Flavor::LiteLlm | Flavor::OpenRouter => OutputShape::Free,
    };
    request.choice_scores = scores;
    serde_json::from_str(&encode_request(&request, flavor).unwrap().0).unwrap()
}

fn ask(k: u32) -> ChoiceScores {
    ChoiceScores::FirstToken { top_k: Count(k) }
}

#[test]
fn off_adds_nothing() {
    for flavor in [Flavor::Vllm, Flavor::LlamaServer] {
        let sent = body(flavor, ChoiceScores::Off);
        assert!(sent.get("logprobs").is_none() && sent.get("top_logprobs").is_none());
    }
}

#[test]
fn vllm_and_llama_server_are_asked_with_the_openai_fields() {
    for flavor in [Flavor::Vllm, Flavor::LlamaServer] {
        let sent = body(flavor, ask(5));
        assert_eq!(sent["logprobs"], json!(true));
        assert_eq!(sent["top_logprobs"], json!(5));
    }
}

#[test]
fn top_k_is_held_between_one_and_twenty() {
    assert_eq!(body(Flavor::Vllm, ask(0))["top_logprobs"], json!(1));
    assert_eq!(body(Flavor::Vllm, ask(500))["top_logprobs"], json!(20));
}

#[test]
fn hosted_gateways_are_not_asked() {
    for flavor in [Flavor::LiteLlm, Flavor::OpenRouter] {
        assert!(body(flavor, ask(5)).get("logprobs").is_none());
    }
}

fn entry(token: &str, logprob: Value) -> Value {
    json!({"token": token, "logprob": logprob, "bytes": null})
}

fn chunk(delta: Value, logprobs: Value, finish: Option<&str>) -> String {
    json!({"id":"c","choices":[{"index":0,"delta":delta,"logprobs":logprobs,"finish_reason":finish}]})
        .to_string()
}

/// A chunk's logprobs: the first logged token and its candidates.
fn logged(token: &str, candidates: &[(&str, Value)]) -> Value {
    let top: Vec<Value> = candidates
        .iter()
        .map(|(t, l)| entry(t, l.clone()))
        .collect();
    json!({"content": [{"token": token, "logprob": -0.1, "bytes": null, "top_logprobs": top}]})
}

fn run(flavor: Flavor, frames: &[String]) -> Option<FirstTokenLogprobs> {
    let mut decoder = StreamDecoder::new(flavor, ModelName("m".into()));
    for frame in frames {
        decoder.feed(frame).unwrap();
    }
    decoder.feed("[DONE]").unwrap();
    decoder.finish().unwrap().first_token
}

fn pass_flag() -> Value {
    logged("pass", &[("pass", json!(-0.2)), ("flag", json!(-1.8))])
}

fn expected() -> Option<FirstTokenLogprobs> {
    let t = |token: &str, nats: f64| TokenLogprob {
        token: token.into(),
        logprob: Logprob::from_nats(nats).unwrap(),
    };
    Some(FirstTokenLogprobs {
        top: vec![t("pass", -0.2), t("flag", -1.8)],
    })
}

#[test]
fn the_first_answer_chunk_gives_the_record_on_both_engines() {
    for flavor in [Flavor::Vllm, Flavor::LlamaServer] {
        let frames = [
            chunk(json!({"role":"assistant","content":""}), Value::Null, None),
            chunk(json!({"content":"pass"}), pass_flag(), None),
            chunk(json!({}), Value::Null, Some("stop")),
        ];
        assert_eq!(run(flavor, &frames), expected());
    }
}

#[test]
fn a_reasoning_model_attributes_the_answers_first_token_not_the_thinking() {
    let thinking = logged("Hmm", &[("Hmm", json!(-0.1)), ("The", json!(-2.0))]);
    let frames = [
        chunk(json!({"reasoning_content":"Hmm"}), thinking, None),
        chunk(json!({"content":"pass"}), pass_flag(), None),
        chunk(json!({}), Value::Null, Some("stop")),
    ];
    assert_eq!(run(Flavor::Vllm, &frames), expected());
}

#[test]
fn a_chunk_that_mixes_thinking_and_answer_is_not_attributed() {
    let frames = [
        chunk(
            json!({"reasoning_content":"ok","content":"pass"}),
            pass_flag(),
            None,
        ),
        chunk(json!({}), Value::Null, Some("stop")),
    ];
    assert_eq!(run(Flavor::Vllm, &frames), None);
}

#[test]
fn a_first_logged_token_that_is_not_the_answers_start_gives_none() {
    let marker = logged("</think>", &[("pass", json!(-0.2))]);
    let frames = [
        chunk(json!({"content":"pass"}), marker, None),
        chunk(json!({}), Value::Null, Some("stop")),
    ];
    assert_eq!(run(Flavor::Vllm, &frames), None);
}

#[test]
fn only_the_first_answer_chunk_counts() {
    let later = logged("flag", &[("flag", json!(-0.1))]);
    let frames = [
        chunk(json!({"content":"pass"}), pass_flag(), None),
        chunk(json!({"content":"ed"}), later, None),
        chunk(json!({}), Value::Null, Some("stop")),
    ];
    assert_eq!(run(Flavor::Vllm, &frames), expected());
}

#[test]
fn missing_or_malformed_logprobs_give_none_and_the_turn_still_ends() {
    let garbled = [
        Value::Null,
        json!({"content": []}),
        json!({"content": "nope"}),
        json!({"content": [{"token": "pass", "top_logprobs": "x"}]}),
        logged("pass", &[("pass", json!("high")), ("flag", Value::Null)]),
    ];
    for logprobs in garbled {
        let frames = [
            chunk(json!({"content":"pass"}), logprobs, None),
            chunk(json!({}), Value::Null, Some("stop")),
        ];
        assert_eq!(run(Flavor::Vllm, &frames), None);
    }
}

#[test]
fn a_gateway_flavor_never_reads_logprobs() {
    let frames = [
        chunk(json!({"content":"pass"}), pass_flag(), None),
        chunk(json!({}), Value::Null, Some("stop")),
    ];
    assert_eq!(run(Flavor::OpenRouter, &frames), None);
}
