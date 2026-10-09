//! The integration tests of `model-openai-compat`, one binary (one link) for all of them.

mod audio_codec;
mod classify;
mod driver;
mod e2e;
mod first_token;
mod hostile;
mod models_embed;
mod request;
mod shapes;
mod speech_describe;
mod speech_fake;
mod speech_stt;
mod speech_tts;
mod stream_decoder;
mod support;

/// Fails when a test file is not declared above: a file left out would silently never run.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let declared = include_str!("main.rs");
    for entry in std::fs::read_dir(&tests).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            path.extension().is_none_or(|e| e != "rs"),
            "{} is a binary of its own; move it under tests/it/",
            path.display()
        );
    }
    for entry in std::fs::read_dir(tests.join("it")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let is_module = path.is_dir() || path.extension().is_some_and(|e| e == "rs");
        if is_module && name != "main" {
            assert!(
                declared.contains(&format!("mod {name};")),
                "tests/it/{name} is not declared in tests/it/main.rs"
            );
        }
    }
}
