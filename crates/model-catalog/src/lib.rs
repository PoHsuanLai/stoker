//! The curated table of models we know how to run, as data files.
//!
//! `catalog/<id>.toml` is installed at `/usr/share/stoker/catalog/` and
//! `$XDG_DATA_HOME/stoker/catalog/`; a user file replaces the system file of the same id. Every
//! field is written and nothing defaults: a file with a missing field is refused.

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
pub use engine::{EngineArg, EngineKind, EngineProfile, FileName, WeightFiles};
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
