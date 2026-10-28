//! The engine a disk word names. The words themselves are `Engine::disk_word` in `engine-spec`:
//! they were written by installs that already exist (`llama_cpp/` folders, records, the install
//! bus method's argument), so llama.cpp's is `llama_cpp`, not the catalog's `llama_server` slug.

use engine_spec::engine_from_disk_word;
use model_catalog::EngineKind;

/// The engine a word names; `None` for any other text.
pub fn engine_from_slug(text: &str) -> Option<EngineKind> {
    engine_from_disk_word(text)
}
