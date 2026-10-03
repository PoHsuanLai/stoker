//! The `embed` table (interface ask 98) and the llama-server weights pairing (ask 66).

use model_catalog::{CatalogError, WeightFiles, parse_entry};
use model_provider::{BatchMax, Dims, EmbedCaps, EmbedPrompts, PrefixText, Tokens};

const EMBED_ONLY: &str = r#"
id = "nomic-embed-text-v1.5"
label = "Nomic Embed Text v1.5"
licence = { kind = "open", v = "Apache-2.0" }
source = { kind = "hugging_face", v = { repo = "nomic-ai/nomic-embed-text-v1.5-GGUF", revision = "0123456789abcdef0123456789abcdef01234567" } }
vram = { weights_mib = 300, kv_per_1k_ctx_mib = 4, overhead_mib = 200 }
roles = ["embeddings"]
embed = { dims = 768, max_batch = 32, max_input = 8192, prompts = { query = "search_query: ", document = "search_document: " } }

[[engine]]
kind = "llama_server"
args = ["--embeddings"]
weights = { kind = "gguf", v = { model = "nomic.gguf", mmproj = "" } }
"#;

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
fn the_embed_table_round_trips_through_toml() {
    let entry = parse_entry(EMBED_ONLY).unwrap();
    let text = toml::to_string(&entry).unwrap();
    assert_eq!(parse_entry(&text).unwrap(), entry);
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
