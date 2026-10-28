//! The word for an engine on a disk, in a record and in a helper's arguments. These words were
//! written by installs that already exist (`llama_cpp/` folders, records, the install bus
//! method's argument), so they stay as they are: llama.cpp's is `llama_cpp`, not the catalog's
//! `llama_server` slug. The other engines' words are their catalog slugs.

use model_catalog::EngineKind;

/// The word for `engine` in a directory name, a record and a helper's arguments.
pub fn engine_slug(engine: EngineKind) -> &'static str {
    match engine {
        EngineKind::LlamaServer => "llama_cpp",
        EngineKind::Vllm => "vllm",
        EngineKind::Strata => "strata",
        EngineKind::SpeechHost => "speech_host",
        EngineKind::KokoroFastApi => "kokoro_fast_api",
    }
}

/// The engine a word names; `None` for any other text.
pub fn engine_from_slug(text: &str) -> Option<EngineKind> {
    [
        EngineKind::LlamaServer,
        EngineKind::Vllm,
        EngineKind::Strata,
        EngineKind::SpeechHost,
        EngineKind::KokoroFastApi,
    ]
    .into_iter()
    .find(|engine| engine_slug(*engine) == text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slug_names_one_engine_and_nothing_else_does() {
        for engine in [
            EngineKind::LlamaServer,
            EngineKind::Vllm,
            EngineKind::Strata,
        ] {
            assert_eq!(engine_from_slug(engine_slug(engine)), Some(engine));
        }
        assert_eq!(engine_slug(EngineKind::LlamaServer), "llama_cpp");
        for text in [
            "",
            "LlamaServer",
            "llama.cpp",
            "llama_server",
            "../vllm",
            "vllm ",
            "https://x/y",
        ] {
            assert_eq!(engine_from_slug(text), None, "{text:?}");
        }
    }
}
