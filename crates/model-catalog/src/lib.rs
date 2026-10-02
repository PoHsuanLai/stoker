//! The curated table of models we know how to run, as data files.
//!
//! `catalog/<id>.toml` is installed at `/usr/share/stoker/catalog/` and
//! `$XDG_DATA_HOME/stoker/catalog/`; a user file replaces the system file of the same id. Every
//! field is written and nothing defaults: a file with a missing field is refused.

mod engine;
mod entry;
mod parse;

pub use engine::{EngineArg, EngineKind, EngineProfile, FileName, WeightFiles};
pub use entry::{
    CatalogId, CatalogKind, GitRevision, GpuNeed, HfRepo, Licence, MiB, ModelEntry, Spdx,
    VramEstimate, WeightSource,
};
pub use parse::{CatalogError, merge_catalogs, parse_entry};
