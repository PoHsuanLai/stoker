//! `describe`, `parse_models`, `encode_embed` and `decode_embed`.

use model_http::{Framing, RouteRoot, Verb};
use model_openai_compat::{CodecError, Flavor, OpenAiCodec};
use model_provider::{Dims, EmbedRole, EmbedTurn, EmbedVector, Knob, ModelInfo, ModelName, Tokens};
use model_wire::{ChatCodec, EmbedCodec};
use serde_json::{Value, json};

fn info(name: &str, loaded: u32, trained: u32) -> ModelInfo {
    ModelInfo {
        name: ModelName(name.into()),
        loaded_context: Tokens(loaded),
        trained_context: Tokens(trained),
    }
}

fn models(flavor: Flavor, body: Value) -> Result<Vec<ModelInfo>, CodecError> {
    OpenAiCodec::new(flavor).parse_models(body.to_string().as_bytes())
}

#[test]
fn describe_reads_props_at_the_server_root_for_llama_server_and_models_elsewhere() {
    let llama = OpenAiCodec::new(Flavor::LlamaServer).describe();
    assert_eq!(
        (llama.verb, llama.root, llama.path.0.as_str(), llama.framing),
        (Verb::Get, RouteRoot::Server, "/props", Framing::Whole)
    );
    for flavor in [Flavor::Vllm, Flavor::LiteLlm, Flavor::OpenRouter] {
        let ex = OpenAiCodec::new(flavor).describe();
        assert_eq!(
            (ex.verb, ex.root, ex.path.0.as_str(), ex.body),
            (Verb::Get, RouteRoot::Base, "/models", None),
            "{flavor:?}"
        );
    }
}

#[test]
fn vllm_reports_max_model_len() {
    let body = json!({"object": "list", "data": [
        {"id": "Qwen/Qwen3-4B", "object": "model", "max_model_len": 32768, "root": "x"},
        {"id": "other", "object": "model", "max_model_len": 4096},
    ]});
    assert_eq!(
        models(Flavor::Vllm, body).unwrap(),
        vec![
            info("Qwen/Qwen3-4B", 32768, 32768),
            info("other", 4096, 4096)
        ]
    );
}

#[test]
fn llama_server_props_report_the_loaded_context_per_slot() {
    let body = json!({
        "default_generation_settings": {"id": 0, "n_ctx": 8192, "params": {}},
        "total_slots": 1,
        "model_path": "/home/me/models/holo-3.1-4b-q4_k_m.gguf",
    });
    assert_eq!(
        models(Flavor::LlamaServer, body).unwrap(),
        vec![info("holo-3.1-4b-q4_k_m.gguf", 8192, 8192)]
    );
    let aliased = json!({
        "default_generation_settings": {"n_ctx": 4096},
        "model_alias": "holo",
        "model_path": "/m/x.gguf",
    });
    assert_eq!(
        models(Flavor::LlamaServer, aliased).unwrap(),
        vec![info("holo", 4096, 4096)]
    );
}

#[test]
fn a_models_list_with_a_trained_context_keeps_both() {
    let body = json!({"data": [
        {"id": "m", "meta": {"n_ctx_train": 40960}, "max_model_len": 8192},
        {"id": "router", "context_length": 128000},
    ]});
    assert_eq!(
        models(Flavor::OpenRouter, body).unwrap(),
        vec![info("m", 8192, 40960), info("router", 128000, 128000)]
    );
}

#[test]
fn a_server_that_reports_no_context_gives_zero() {
    let body = json!({"data": [{"id": "gpt-x", "object": "model"}]});
    assert_eq!(
        models(Flavor::LiteLlm, body).unwrap(),
        vec![info("gpt-x", 0, 0)]
    );
}

#[test]
fn an_unreadable_model_list_is_unreadable() {
    let cases = [
        json!({"data": [{"object": "model"}]}),
        json!({"data": [{"id": ""}]}),
        json!({"data": [7]}),
        json!({"nothing": true}),
        json!({"default_generation_settings": {"n_ctx": 1}}),
        json!([1, 2]),
    ];
    for body in cases {
        assert_eq!(
            models(Flavor::Vllm, body.clone()),
            Err(CodecError::Unreadable),
            "{body}"
        );
    }
    assert_eq!(
        OpenAiCodec::new(Flavor::Vllm).parse_models(b"<html>"),
        Err(CodecError::Unreadable)
    );
    assert_eq!(models(Flavor::Vllm, json!({"data": []})).unwrap(), vec![]);
}

fn turn(inputs: &[&str], dims: Knob<Dims>) -> EmbedTurn {
    EmbedTurn {
        model: ModelName("nomic".into()),
        inputs: inputs.iter().map(|s| (*s).to_owned()).collect(),
        role: EmbedRole::Query,
        dims,
    }
}

fn embed_body(flavor: Flavor, turn: &EmbedTurn) -> Value {
    let ex = OpenAiCodec::new(flavor).encode_embed(turn).unwrap();
    assert_eq!(
        (ex.verb, ex.root, ex.path.0.as_str(), ex.framing),
        (
            Verb::PostJson,
            RouteRoot::Base,
            "/embeddings",
            Framing::Whole
        )
    );
    serde_json::from_str(&ex.body.unwrap().0).unwrap()
}

#[test]
fn the_embedding_request_is_the_golden_body() {
    assert_eq!(
        embed_body(Flavor::Vllm, &turn(&["search_query: a", "b"], Knob::Off)),
        json!({"model": "nomic", "input": ["search_query: a", "b"], "encoding_format": "float"})
    );
}

#[test]
fn dimensions_are_sent_only_where_the_server_honours_them() {
    let asked = turn(&["a"], Knob::Set(Dims(256)));
    for flavor in [Flavor::Vllm, Flavor::LiteLlm, Flavor::OpenRouter] {
        assert_eq!(
            embed_body(flavor, &asked)["dimensions"],
            json!(256),
            "{flavor:?}"
        );
    }
    assert!(
        embed_body(Flavor::LlamaServer, &asked)
            .get("dimensions")
            .is_none()
    );
    assert!(
        embed_body(Flavor::Vllm, &turn(&["a"], Knob::Off))
            .get("dimensions")
            .is_none()
    );
}

fn decode(body: Value) -> Result<model_provider::EmbedEnd, CodecError> {
    OpenAiCodec::new(Flavor::Vllm)
        .decode_embed(ModelName("nomic".into()), body.to_string().as_bytes())
}

#[test]
fn the_vectors_come_back_in_index_order() {
    let end = decode(json!({
        "object": "list",
        "model": "nomic",
        "data": [
            {"object": "embedding", "index": 1, "embedding": [3.0, 4.0]},
            {"object": "embedding", "index": 0, "embedding": [1.0, 2.5]},
        ],
        "usage": {"prompt_tokens": 7, "total_tokens": 7},
    }))
    .unwrap();
    assert_eq!(
        end.vectors,
        vec![EmbedVector(vec![1.0, 2.5]), EmbedVector(vec![3.0, 4.0])]
    );
    assert_eq!(end.usage.input, Tokens(7));
    assert_eq!(end.served, ModelName("nomic".into()));
}

#[test]
fn a_reply_with_no_index_is_in_reply_order() {
    let end = decode(json!({"data": [{"embedding": [1]}, {"embedding": [2]}]})).unwrap();
    assert_eq!(
        end.vectors,
        vec![EmbedVector(vec![1.0]), EmbedVector(vec![2.0])]
    );
    assert_eq!(end.usage.input, Tokens(0));
}

#[test]
fn a_hole_a_repeat_or_a_non_number_is_unreadable() {
    let cases = [
        json!({"data": [{"index": 1, "embedding": [1]}]}),
        json!({"data": [{"index": 0, "embedding": [1]}, {"index": 0, "embedding": [2]}]}),
        json!({"data": [{"index": 0, "embedding": [1, "x"]}]}),
        json!({"data": [{"index": 0, "embedding": "AAAA"}]}),
        json!({"data": [{"index": -1, "embedding": [1]}]}),
        json!({"data": [{"index": 0}]}),
        json!({"nothing": 1}),
    ];
    for body in cases {
        assert_eq!(decode(body.clone()), Err(CodecError::Unreadable), "{body}");
    }
    assert_eq!(
        OpenAiCodec::new(Flavor::Vllm).decode_embed(ModelName("m".into()), b"nope"),
        Err(CodecError::Unreadable)
    );
}

#[test]
fn an_empty_data_array_is_no_vectors() {
    assert!(decode(json!({"data": []})).unwrap().vectors.is_empty());
}

#[test]
fn any_permutation_of_indexes_is_put_back_in_order() {
    use proptest::prelude::*;
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&proptest::collection::vec(any::<u8>(), 1..12), |seeds| {
            let n = seeds.len();
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by_key(|i| (seeds[*i], *i));
            let data: Vec<Value> = order
                .iter()
                .map(|i| json!({"index": i, "embedding": [*i as f32]}))
                .collect();
            let end = decode(json!({"data": data})).unwrap();
            let got: Vec<f32> = end.vectors.iter().map(|v| v.0[0]).collect();
            prop_assert_eq!(got, (0..n).map(|i| i as f32).collect::<Vec<_>>());
            Ok(())
        })
        .unwrap();
}
