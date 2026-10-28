//! The trait: what one engine does differently from the others.

use std::path::Path;

use model_catalog::{EngineArg, EngineKind, ModelEntry, WeightFiles};

use crate::host_env::HostEnv;
use crate::package::PackageSource;

/// What one engine does differently. Every method has a default, so an engine overrides only
/// what differs; [`engine`] is the one place that maps an [`EngineKind`] to its implementation.
pub trait Engine: Sync {
    /// The catalog slug (`llama_server`): the `EngineKind` serde form.
    fn slug(&self) -> &'static str;

    /// The word in install folders, records and the helper's arguments. Those words were written
    /// by installs that already exist, so llama.cpp's stays `llama_cpp`. Default: the slug.
    fn disk_word(&self) -> &'static str {
        self.slug()
    }

    /// The id the engine goes by where a person's settings and the bus name it (`llama_cpp`,
    /// `vllm`, `strata`, `speech`, `kokoro`). Those words are already on the bus, so the speech
    /// engines keep the short ones. Default: the disk word.
    fn bus_word(&self) -> &'static str {
        self.disk_word()
    }

    /// The word a list of running models shows for the engine (`llama-server`, `speech-host`: the
    /// program's own name where it has one). Default: the slug.
    fn running_word(&self) -> &'static str {
        self.slug()
    }

    /// The distro packages that hold this engine, with the releases each was checked on.
    fn packages(&self) -> &'static [PackageSource] {
        &[]
    }

    /// The fixed paths a distro package puts the program at, first choice first.
    fn package_paths(&self) -> &'static [&'static str] {
        &[]
    }

    /// How to recognise a folder the person points at, for an engine that is run from one.
    fn pointed(&self) -> Option<Pointed> {
        None
    }

    /// The arguments before and after the profile's own, for one launch.
    fn command(&self, _launch: &Launch<'_>) -> LaunchArgs {
        LaunchArgs::default()
    }

    /// The launch-argument names a profile sets how many requests the engine takes at once with
    /// (`--max-num-seqs`), when it takes them in turn at all. `None` for an engine that takes
    /// every request at once.
    fn seat_flags(&self) -> Option<&'static [&'static str]> {
        None
    }

    /// What the engine's unit gets besides its command line. `libs` is the folder of the shared
    /// libraries the engine loads, when the person set one. Default: nothing.
    fn env(&self, _libs: Option<&Path>) -> HostEnv {
        HostEnv::default()
    }

    /// Whether the engine keeps the prompt of a conversation between requests.
    fn caches_prompts(&self) -> bool {
        false
    }

    /// Whether the engine runs speech-to-text models. A model of neither speech kind runs on an
    /// engine that does neither.
    fn hears(&self) -> bool {
        false
    }

    /// Whether the engine runs text-to-speech models.
    fn speaks(&self) -> bool {
        false
    }

    /// Whether this engine reads `weights`. Default: any.
    fn accepts(&self, _weights: &WeightFiles) -> bool {
        true
    }

    /// Whether an entry served by this engine must carry a GPU memory estimate.
    fn requires_gpu_estimate(&self) -> bool {
        false
    }
}

/// The five engines, once each: the set `engine_from_slug`-style lookups run over.
pub const ENGINES: [EngineKind; 5] = [
    EngineKind::LlamaServer,
    EngineKind::Vllm,
    EngineKind::Strata,
    EngineKind::SpeechHost,
    EngineKind::KokoroFastApi,
];

/// The implementation of `kind`. The only `match` on `EngineKind` that picks behaviour.
pub fn engine(kind: EngineKind) -> &'static dyn Engine {
    match kind {
        EngineKind::LlamaServer => &crate::llama::Llama,
        EngineKind::Vllm => &crate::vllm::Vllm,
        EngineKind::Strata => &crate::strata::Strata,
        EngineKind::SpeechHost => &crate::speech_host::SpeechHost,
        EngineKind::KokoroFastApi => &crate::kokoro::Kokoro,
    }
}

/// What a launch gives [`Engine::command`].
#[derive(Debug, Clone, Copy)]
pub struct Launch<'a> {
    /// The model being started.
    pub entry: &'a ModelEntry,
    /// The weights of the profile being started.
    pub weights: &'a WeightFiles,
    /// The directory of the entry's snapshot under the weights cache.
    pub snapshot: &'a Path,
    /// The Unix socket the engine listens on, as text.
    pub socket: &'a str,
}

/// An engine's own arguments: `front` goes before the profile's `args`, `back` between them and
/// the context window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    pub front: Vec<EngineArg>,
    pub back: Vec<EngineArg>,
}

/// Where the parts of an engine sit inside a folder the person points at. A folder counts when
/// it holds `marker` and an executable `program`; the release is read from `build_record`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointed {
    pub program: &'static str,
    pub marker: &'static str,
    pub build_record: &'static str,
}

/// The engine whose disk word is `word`; `None` for any other text.
pub fn engine_from_disk_word(word: &str) -> Option<EngineKind> {
    ENGINES
        .into_iter()
        .find(|kind| engine(*kind).disk_word() == word)
}

pub(crate) fn own(args: &[&str]) -> Vec<EngineArg> {
    args.iter().map(|a| EngineArg((*a).to_owned())).collect()
}

pub(crate) fn text_arg(text: impl Into<String>) -> EngineArg {
    EngineArg(text.into())
}
