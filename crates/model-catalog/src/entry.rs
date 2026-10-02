//! One model's entry.

use std::collections::BTreeSet;

use model_provider::{Caps, Tokens};
use serde::{Deserialize, Serialize};

use crate::EngineProfile;

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

impl VramEstimate {
    /// `weights + kv(context) + overhead`; the KV share rounds up to whole mebibytes.
    pub fn need(&self, context: Tokens) -> MiB {
        let kv = (u64::from(self.kv_per_1k_ctx.0) * u64::from(context.0)).div_ceil(1000);
        let total = u64::from(self.weights.0) + kv + u64::from(self.overhead.0);
        MiB(u32::try_from(total).unwrap_or(u32::MAX))
    }
}

/// The kinds of AI work a model is offered for. The catalog's own copy of porter's `AiKind`
/// (stoker does not depend on porter); inferd's bridge maps one to the other, and the two
/// slugs are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Llm,
    ComputerUse,
    Embeddings,
    Speech,
    ImageGen,
    Rerank,
}

/// The file's serde form: the capability fields sit at the top level beside the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: CatalogId,
    pub label: String,
    pub licence: Licence,
    pub source: WeightSource,
    pub vram: VramEstimate,
    pub roles: BTreeSet<CatalogKind>,
    #[serde(flatten)]
    pub caps: Caps,
    #[serde(rename = "engine")]
    pub engines: Vec<EngineProfile>,
}
