//! The words and facts each engine answers with.

use engine_spec::{ENGINES, engine, engine_from_disk_word};
use model_catalog::EngineKind;

#[test]
fn a_disk_word_names_one_engine_and_nothing_else_does() {
    for kind in ENGINES {
        assert_eq!(engine_from_disk_word(engine(kind).disk_word()), Some(kind));
    }
    assert_eq!(engine(EngineKind::LlamaServer).disk_word(), "llama_cpp");
    for text in [
        "",
        "LlamaServer",
        "llama.cpp",
        "llama_server",
        "../vllm",
        "vllm ",
        "https://x/y",
    ] {
        assert_eq!(engine_from_disk_word(text), None, "{text:?}");
    }
}

#[test]
fn a_slug_is_the_serde_form_of_the_kind() {
    for kind in ENGINES {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{}\"", engine(kind).slug()));
    }
}

#[test]
fn every_disk_word_is_the_slug_but_llama_cpps() {
    let words: Vec<_> = ENGINES
        .into_iter()
        .map(|kind| (engine(kind).slug(), engine(kind).disk_word()))
        .collect();
    assert_eq!(
        words,
        [
            ("llama_server", "llama_cpp"),
            ("vllm", "vllm"),
            ("strata", "strata"),
            ("speech_host", "speech_host"),
            ("kokoro_fast_api", "kokoro_fast_api"),
        ]
    );
}

#[test]
fn only_llama_has_distro_packages_and_only_llama_and_vllm_have_package_paths() {
    let packaged: Vec<_> = ENGINES
        .into_iter()
        .filter(|kind| !engine(*kind).packages().is_empty())
        .collect();
    assert_eq!(packaged, [EngineKind::LlamaServer]);
    let on_path: Vec<_> = ENGINES
        .into_iter()
        .filter(|kind| !engine(*kind).package_paths().is_empty())
        .collect();
    assert_eq!(on_path, [EngineKind::LlamaServer, EngineKind::Vllm]);
}

#[test]
fn only_strata_is_run_from_a_folder_the_person_points_at() {
    let pointed: Vec<_> = ENGINES
        .into_iter()
        .filter(|kind| engine(*kind).pointed().is_some())
        .collect();
    assert_eq!(pointed, [EngineKind::Strata]);
}
