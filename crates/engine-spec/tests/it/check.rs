//! `check_entry`: what an entry needs of its engines (moved here from model-catalog's parser).

use engine_spec::{SpecError, check_entry};
use model_catalog::{EngineKind, FileName, MiB, ModelEntry, WeightFiles, parse_entry};

const CATALOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalog");

fn shipped(id: &str) -> ModelEntry {
    let text = std::fs::read_to_string(format!("{CATALOG}/{id}.toml")).unwrap();
    parse_entry(&text).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn every_shipped_entry_passes() {
    for file in std::fs::read_dir(CATALOG).unwrap() {
        let path = file.unwrap().path();
        let entry = parse_entry(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(check_entry(&entry), Ok(()), "{}", path.display());
    }
}

#[test]
fn a_vllm_engine_with_an_all_zero_estimate_is_a_mistake_in_the_file() {
    let mut entry = shipped("holo-3.1-4b");
    assert_eq!(entry.engines[0].kind, EngineKind::Vllm);
    entry.vram.weights = MiB(0);
    entry.vram.kv_per_1k_ctx = MiB(0);
    entry.vram.overhead = MiB(0);
    assert_eq!(
        check_entry(&entry),
        Err(SpecError::GpuEstimateMissing { engine: "vllm" })
    );
    // An engine that does not need the share is not asked for one.
    entry.engines[0].kind = EngineKind::SpeechHost;
    assert_eq!(check_entry(&entry), Ok(()));
}

#[test]
fn a_llama_server_profile_must_name_gguf_weights() {
    let mut entry = shipped("holo-3.1-4b");
    entry.engines[0].kind = EngineKind::LlamaServer;
    for weights in [WeightFiles::HfSnapshot, WeightFiles::SherpaDir] {
        entry.engines[0].weights = weights.clone();
        assert_eq!(
            check_entry(&entry),
            Err(SpecError::WeightsNotAccepted {
                engine: "llama_server"
            }),
            "{weights:?}"
        );
    }
    entry.engines[0].weights = WeightFiles::Gguf {
        model: FileName("a.gguf".into()),
        mmproj: None,
    };
    assert_eq!(check_entry(&entry), Ok(()));
    // vLLM keeps reading snapshots.
    entry.engines[0].kind = EngineKind::Vllm;
    entry.engines[0].weights = WeightFiles::HfSnapshot;
    assert_eq!(check_entry(&entry), Ok(()));
}
