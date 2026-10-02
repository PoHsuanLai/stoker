//! How an engine is started for one model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    LlamaServer,
    Vllm,
}

/// One command-line argument, passed to the engine as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineArg(pub String);

/// A file name inside a weights directory (no directory part).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileName(pub String);

/// Where an engine finds the weights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum WeightFiles {
    /// A Hugging Face snapshot directory, as vLLM reads it.
    HfSnapshot,
    /// A GGUF model and its multimodal projector, as llama-server reads them.
    Gguf { model: FileName, mmproj: FileName },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineProfile {
    pub kind: EngineKind,
    pub args: Vec<EngineArg>,
    pub weights: WeightFiles,
}
