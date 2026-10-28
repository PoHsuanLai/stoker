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

#[test]
fn the_bus_and_running_words_are_the_ones_already_on_the_bus() {
    let words: Vec<_> = ENGINES
        .into_iter()
        .map(|kind| (engine(kind).bus_word(), engine(kind).running_word()))
        .collect();
    assert_eq!(
        words,
        [
            ("llama_cpp", "llama-server"),
            ("vllm", "vllm"),
            ("strata", "strata"),
            ("speech", "speech-host"),
            ("kokoro", "kokoro"),
        ]
    );
}

#[test]
fn seats_prompt_caches_and_speech_are_facts_of_the_engine() {
    let seats: Vec<_> = ENGINES
        .into_iter()
        .map(|kind| engine(kind).seat_flags())
        .collect();
    assert_eq!(
        seats,
        [
            Some(&["--parallel", "-np"][..]),
            Some(&["--max-num-seqs"][..]),
            None,
            None,
            None
        ]
    );
    let caching: Vec<_> = ENGINES
        .into_iter()
        .filter(|kind| engine(*kind).caches_prompts())
        .collect();
    assert_eq!(caching, [EngineKind::LlamaServer]);
    let speech: Vec<_> = ENGINES
        .into_iter()
        .map(|kind| (engine(kind).hears(), engine(kind).speaks()))
        .collect();
    assert_eq!(
        speech,
        [
            (false, false),
            (false, false),
            (false, false),
            (true, true),
            (false, true)
        ]
    );
}

#[test]
fn only_vllm_and_the_speech_host_add_to_their_unit() {
    use engine_spec::HostEnv;
    use std::path::Path;
    let libs = Path::new("/opt/sherpa/lib");
    let vllm = engine(EngineKind::Vllm).env(Some(libs));
    assert_eq!(vllm.vars, [("PYTHONDONTWRITEBYTECODE", "1".to_owned())]);
    assert!(vllm.reads.is_empty());
    let host = engine(EngineKind::SpeechHost);
    let with = host.env(Some(libs));
    assert_eq!(
        with.vars,
        [("LD_LIBRARY_PATH", "/opt/sherpa/lib".to_owned())]
    );
    assert_eq!(with.reads, [libs.to_path_buf()]);
    assert_eq!(host.env(None), HostEnv::default());
    for kind in [
        EngineKind::LlamaServer,
        EngineKind::Strata,
        EngineKind::KokoroFastApi,
    ] {
        assert_eq!(engine(kind).env(Some(libs)), HostEnv::default());
    }
}
