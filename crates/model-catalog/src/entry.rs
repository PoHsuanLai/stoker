//! One model's entry.

use std::collections::BTreeSet;

use model_provider::{Caps, EmbedCaps, Reasoning, Sampling, Tokens};
use serde::{Deserialize, Serialize};
use speech_provider::{SpeechCaps, SpeechDir};

use crate::{Capabilities, EngineProfile};

/// The id of a catalog entry, and the file's stem: `holo-3.1-4b`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CatalogId(pub String);

/// An SPDX licence expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Spdx(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Licence {
    Open(Spdx),
    /// Listed in the picker, never chosen automatically.
    NonCommercial(Spdx),
    Proprietary,
}

/// The model family a weights set belongs to (`qwen`, `whisper`): lowercase, the same string for
/// every size and fine-tune of one lineage. A router uses it for the "a reviewer of a different
/// family" rule; it says nothing about quality.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Family(pub String);

/// An estimate, in whole seconds, of how long a cold engine takes from start to ready (weights
/// read, engine warm-up). It is a figure written by hand from the engine kind, not a measurement;
/// a router only compares it to other estimates and never treats it as a promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColdStartEstimateS(pub u16);

/// A Hugging Face repository, `Org/Name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HfRepo(pub String);

/// A full commit hash, so a weights directory is one exact set of files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GitRevision(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum WeightSource {
    HuggingFace { repo: HfRepo, revision: GitRevision },
}

/// Mebibytes of memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MiB(pub u32);

/// What a model costs in GPU memory: weights, KV cache per thousand tokens of context, and the
/// engine's own overhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VramEstimate {
    #[serde(rename = "weights_mib")]
    pub weights: MiB,
    #[serde(rename = "kv_per_1k_ctx_mib")]
    pub kv_per_1k_ctx: MiB,
    #[serde(rename = "overhead_mib")]
    pub overhead: MiB,
}

/// Whether an engine for a model needs the GPU at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuNeed {
    /// The estimate is all zeros: the engine runs on the CPU, and its sandbox has no GPU access.
    Absent,
    Needed,
}

impl VramEstimate {
    /// `Absent` exactly when weights, KV and overhead are all zero.
    pub fn gpu_need(&self) -> GpuNeed {
        match (self.weights, self.kv_per_1k_ctx, self.overhead) {
            (MiB(0), MiB(0), MiB(0)) => GpuNeed::Absent,
            _ => GpuNeed::Needed,
        }
    }

    /// `weights + kv(context) + overhead`; the KV share rounds up to whole mebibytes.
    pub fn need(&self, context: Tokens) -> MiB {
        let kv = (u64::from(self.kv_per_1k_ctx.0) * u64::from(context.0)).div_ceil(1000);
        let total = u64::from(self.weights.0) + kv + u64::from(self.overhead.0);
        MiB(u32::try_from(total).unwrap_or(u32::MAX))
    }
}

/// The kinds of AI work a model is offered for. Deprecated: use `Slot` (the slots are derived from
/// capabilities; this set is derived the same way). The catalog's own copy of porter's `AiKind`
/// (stoker does not depend on porter); inferd's bridge maps one to the other, and the two
/// slugs are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Llm,
    ComputerUse,
    Embeddings,
    /// Speech to text (porter's `AiKind` splits the old `Speech` row the same way).
    SpeechIn,
    /// Text to speech.
    SpeechOut,
    ImageGen,
    Rerank,
}

impl CatalogKind {
    /// The direction of a speech role; `None` for every chat-side role.
    pub fn speech_dir(self) -> Option<SpeechDir> {
        match self {
            CatalogKind::SpeechIn => Some(SpeechDir::In),
            CatalogKind::SpeechOut => Some(SpeechDir::Out),
            CatalogKind::Llm
            | CatalogKind::ComputerUse
            | CatalogKind::Embeddings
            | CatalogKind::ImageGen
            | CatalogKind::Rerank => None,
        }
    }
}

/// A model's default sampling, one set per reasoning mode: Qwen-family models want different
/// temperatures with thinking on and off. Written in full in every chat entry; a request carries
/// the values it uses, taken from here when the caller has no opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingDefaults {
    pub reasoning_on: Sampling,
    pub reasoning_off: Sampling,
    /// What the model does when a request says nothing about reasoning (`Reasoning::EngineDefault`:
    /// the codec sends no switch, so the model's chat template decides). Written in every chat
    /// entry so that the sampling a caller takes for that case is the one the model really runs
    /// with.
    pub reasoning_default: ReasoningDefault,
}

/// Whether a model thinks when nothing asks it to or not to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningDefault {
    On,
    Off,
}

impl SamplingDefaults {
    /// The sampling for a request's reasoning setting: `On(_)` takes `reasoning_on`, `Off` takes
    /// `reasoning_off`, and `EngineDefault` takes whichever the model's `reasoning_default` names.
    pub fn for_reasoning(&self, reasoning: Reasoning) -> &Sampling {
        match reasoning {
            Reasoning::On(_) => &self.reasoning_on,
            Reasoning::Off => &self.reasoning_off,
            Reasoning::EngineDefault => match self.reasoning_default {
                ReasoningDefault::On => &self.reasoning_on,
                ReasoningDefault::Off => &self.reasoning_off,
            },
        }
    }
}

/// One entry of the catalogue.
///
/// `capabilities` is the truth: what the model takes in and gives out, with a detail table for
/// each declared modality. The slots a person picks from are derived from it (`slot_members`).
///
/// `roles`, `caps`, `sampling`, `speech` and `embed` are the older view that porter's inferd still
/// reads; they are derived from `capabilities` when the file is read (`parse_entry`), never
/// declared beside it. Deprecated: use `Slot`. They go once porter reads slots. `caps` exists when
/// a chat role does (`llm`, `computer_use`), `speech` is the audio table (in preferred), `embed`
/// the vector table.
///
/// An entry in the older file shape (`roles`, `speech`, `embed`, chat fields at the top) is read
/// into the same struct: its `roles` and chat fields are kept as written and its capabilities are
/// converted from them. `Serialize` always writes the new shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: CatalogId,
    pub label: String,
    pub licence: Licence,
    pub family: Family,
    /// An estimate, not a measurement (see `ColdStartEstimateS`).
    pub cold_start_estimate_s: ColdStartEstimateS,
    pub source: WeightSource,
    pub vram: VramEstimate,
    pub capabilities: Capabilities,
    /// Deprecated: use `Slot`.
    pub roles: BTreeSet<CatalogKind>,
    /// Deprecated: use `capabilities`.
    pub caps: Option<Caps>,
    /// Deprecated: use `capabilities.text_out`.
    pub sampling: Option<SamplingDefaults>,
    /// Deprecated: use `capabilities.audio_in` and `audio_out`.
    pub speech: Option<SpeechCaps>,
    /// Deprecated: use `capabilities.vector_out`.
    pub embed: Option<EmbedCaps>,
    pub engines: Vec<EngineProfile>,
}
