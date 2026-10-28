//! The checks a catalog entry must pass against the engines it names.

use model_catalog::{GpuNeed, ModelEntry};

use crate::engine::engine;

/// Why an entry does not suit its engines.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SpecError {
    #[error("engine {engine} needs a GPU memory estimate; the entry's is all zeros")]
    GpuEstimateMissing { engine: &'static str },
    #[error("engine {engine} cannot read the weights the profile names")]
    WeightsNotAccepted { engine: &'static str },
}

/// Pure: whether each engine profile of `entry` is one its engine runs: an engine that needs a
/// GPU share has an estimate, and the weights are a kind the engine reads (`command` has no
/// `--model` for anything else). Run it after `parse_entry`.
pub fn check_entry(entry: &ModelEntry) -> Result<(), SpecError> {
    for profile in &entry.engines {
        let spec = engine(profile.kind);
        if spec.requires_gpu_estimate() && entry.vram.gpu_need() == GpuNeed::Absent {
            return Err(SpecError::GpuEstimateMissing {
                engine: spec.slug(),
            });
        }
    }
    for profile in &entry.engines {
        let spec = engine(profile.kind);
        if !spec.accepts(&profile.weights) {
            return Err(SpecError::WeightsNotAccepted {
                engine: spec.slug(),
            });
        }
    }
    Ok(())
}
