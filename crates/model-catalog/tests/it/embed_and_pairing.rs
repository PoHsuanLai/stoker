//! The `embed` table (interface ask 98) and the llama-server weights pairing (ask 66).

use crate::support;

use model_catalog::{CatalogError, WeightFiles, parse_entry};
use model_provider::{BatchMax, Dims, EmbedCaps, EmbedPrompts, PrefixText, Tokens};
use support::EMBED_ONLY;

fn caps() -> EmbedCaps {
    EmbedCaps {
        dims: Dims(768),
        max_batch: BatchMax(32),
        max_input: Tokens(8192),
        prompts: EmbedPrompts {
            query: PrefixText("search_query: ".into()),
            document: PrefixText("search_document: ".into()),
        },
    }
}

#[test]
fn an_embeddings_entry_with_an_embed_table_needs_no_chat_fields() {
    let entry = parse_entry(EMBED_ONLY).unwrap();
    assert_eq!(entry.embed, Some(caps()));
    assert_eq!(entry.caps, None);
    assert_eq!(entry.sampling, None);
    assert_eq!(entry.speech, None);
}

#[test]
fn the_shipped_chat_and_speech_entries_write_no_embed_table() {
    for file in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalog")).unwrap() {
        let text = std::fs::read_to_string(file.unwrap().path()).unwrap();
        assert_eq!(parse_entry(&text).unwrap().embed, None);
    }
}

#[test]
fn an_embeddings_role_without_the_table_still_needs_the_chat_fields() {
    let without: String = EMBED_ONLY
        .lines()
        .filter(|l| !l.starts_with("embed "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(parse_entry(&without), Err(CatalogError::Toml(_))));
}

#[test]
fn the_embed_table_needs_the_embeddings_role() {
    let wrong = EMBED_ONLY.replace(r#"roles = ["embeddings"]"#, r#"roles = ["llm"]"#);
    assert_eq!(
        parse_entry(&wrong),
        Err(CatalogError::EmbedTableWithoutEmbeddingsRole)
    );
}

#[test]
fn a_malformed_embed_table_is_refused_by_name() {
    let bad = EMBED_ONLY.replace("dims = 768, ", "");
    let Err(CatalogError::Toml(why)) = parse_entry(&bad) else {
        panic!("a table without dims must be refused");
    };
    assert!(why.contains("dims"), "{why}");
}

#[test]
fn a_gguf_projector_is_optional_and_an_empty_name_is_one_spelling_of_none() {
    use model_catalog::FileName;
    let with = |weights: &str| {
        let text = EMBED_ONLY.replace(
            r#"{ kind = "gguf", v = { model = "nomic.gguf", mmproj = "" } }"#,
            weights,
        );
        parse_entry(&text).unwrap().engines[0].weights.clone()
    };
    let model = FileName("nomic.gguf".into());
    assert_eq!(
        with(r#"{ kind = "gguf", v = { model = "nomic.gguf" } }"#),
        WeightFiles::Gguf {
            model: model.clone(),
            mmproj: None
        }
    );
    assert_eq!(
        with(r#"{ kind = "gguf", v = { model = "nomic.gguf", mmproj = "vision.gguf" } }"#),
        WeightFiles::Gguf {
            model: model.clone(),
            mmproj: Some(FileName("vision.gguf".into()))
        }
    );
    assert_eq!(
        with(r#"{ kind = "gguf", v = { model = "nomic.gguf", mmproj = "" } }"#),
        WeightFiles::Gguf {
            model,
            mmproj: Some(FileName(String::new()))
        }
    );
    // A missing projector is not written back.
    let none = WeightFiles::Gguf {
        model: FileName("a.gguf".into()),
        mmproj: None,
    };
    assert_eq!(
        serde_json::to_string(&none).unwrap(),
        r#"{"kind":"gguf","v":{"model":"a.gguf"}}"#
    );
}

#[test]
fn a_llama_server_profile_must_name_gguf_weights() {
    let entry = parse_entry(EMBED_ONLY).unwrap();
    assert!(matches!(entry.engines[0].weights, WeightFiles::Gguf { .. }));
    for weights in [r#"{ kind = "hf_snapshot" }"#, r#"{ kind = "sherpa_dir" }"#] {
        let bad = EMBED_ONLY.replace(
            r#"{ kind = "gguf", v = { model = "nomic.gguf", mmproj = "" } }"#,
            weights,
        );
        assert_eq!(
            parse_entry(&bad),
            Err(CatalogError::LlamaServerWithoutGguf),
            "{weights}"
        );
    }
    // vLLM keeps reading snapshots.
    let vllm = EMBED_ONLY.replace("llama_server", "vllm").replace(
        r#"{ kind = "gguf", v = { model = "nomic.gguf", mmproj = "" } }"#,
        r#"{ kind = "hf_snapshot" }"#,
    );
    assert!(parse_entry(&vllm).is_ok());
}

#[test]
fn an_older_embeddings_file_converts_to_text_in_and_vector_out() {
    use model_catalog::{Modality, Slot, fits};
    let entry = parse_entry(EMBED_ONLY).unwrap();
    let c = &entry.capabilities;
    assert_eq!(c.inputs, [Modality::Text].into());
    assert_eq!(c.outputs, [Modality::Vector].into());
    assert_eq!(c.vector_out, Some(caps()));
    assert_eq!(c.text_out, None);
    assert!(fits(Slot::Embeddings, c) && !fits(Slot::Text, c));
}
