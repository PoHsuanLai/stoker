//! What differs between the engines, as one trait.
//!
//! [`Engine`] has a default for every method, so an engine overrides only what it does
//! differently: its disk word, its distro packages, its command line, the weights it reads.
//! [`engine`] maps the catalog's [`EngineKind`](model_catalog::EngineKind) (the serde key, which
//! stays the closed set) to its implementation; it is the only `match` on the kind that picks
//! behaviour. A new engine is one `impl Engine` and one arm there.
//!
//! ```
//! use engine_spec::{ENGINES, engine, engine_from_disk_word};
//! use model_catalog::EngineKind;
//!
//! assert_eq!(engine(EngineKind::LlamaServer).disk_word(), "llama_cpp");
//! assert_eq!(engine(EngineKind::Vllm).disk_word(), "vllm");
//! assert_eq!(engine_from_disk_word("llama_cpp"), Some(EngineKind::LlamaServer));
//! assert_eq!(ENGINES.len(), 5);
//! ```

mod cache;
mod check;
mod engine;
mod host_env;
mod kokoro;
mod llama;
mod package;
mod paths;
mod speech_host;
mod strata;
mod vllm;

pub use cache::{repo_dir, snapshot_dir};
pub use check::{SpecError, check_entry};
pub use engine::{ENGINES, Engine, Launch, LaunchArgs, Pointed, engine, engine_from_disk_word};
pub use host_env::HostEnv;
pub use package::{PackageManager, PackageSource, Releases, Unnumbered, Verified};
pub use paths::{EnginePaths, ProgramPath};
