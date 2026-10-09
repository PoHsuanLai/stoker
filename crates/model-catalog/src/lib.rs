//! The curated table of models we know how to run, as data files.
//!
//! `catalog/<id>.toml` is installed at `/usr/share/stoker/catalog/` and
//! `$XDG_DATA_HOME/stoker/catalog/`; a user file replaces the system file of the same id. Every
//! field is written and nothing defaults: a file with a missing field is refused.
//!
//! ```
//! use model_catalog::parse_entry;
//!
//! let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalog/holo-3.1-4b.toml");
//! let entry = parse_entry(&std::fs::read_to_string(path).unwrap()).unwrap();
//! assert_eq!(entry.id.0, "holo-3.1-4b");
//! // A file with a missing field is refused.
//! assert!(parse_entry("id = \"nothing-else\"").is_err());
//! ```

mod capabilities;
mod check;
mod engine;
mod entry;
mod file;
mod legacy;
mod modality;
mod parse;
mod reach;
mod slot;
mod view;

pub use capabilities::{Capabilities, DetailTable, Side, TextOut};
pub use engine::{
    AttachedEngine, EngineArg, EngineKind, EngineProfile, FileName, ParserName, ServedName,
    Serving, WeightFiles,
};
pub use entry::{
    CatalogId, CatalogKind, ColdStartEstimateS, Family, GitRevision, GpuNeed, HfRepo, Licence,
    Locality, MiB, MicroUsd, ModelEntry, Price, ProviderId, Reach, ReasoningDefault, RemoteModelId,
    SamplingDefaults, Spdx, VramEstimate, WeightSource, Wire,
};
pub use file::EntryFile;
pub use modality::{Modalities, Modality};
pub use parse::{CatalogError, merge_catalogs, parse_entry};
pub use reach::reachable;
pub use slot::{Signature, Slot, ToolNeed, fits, slot_members};
