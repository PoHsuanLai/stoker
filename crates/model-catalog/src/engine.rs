//! How an engine is started for one model.

use serde::{Deserialize, Serialize};

use crate::Modalities;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    LlamaServer,
    Vllm,
    /// Our own STT engine process (`speech-host`, sherpa-onnx without TTS), on the CPU.
    SpeechHost,
    /// Kokoro-FastAPI in a uv environment: a separate process, so its GPL espeak-ng is never
    /// linked into ours.
    KokoroFastApi,
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
    /// A GGUF model and, when it takes images, its multimodal projector, as llama-server reads
    /// them. A text-only or embedding model writes no `mmproj` (an empty name counts as none,
    /// which is how earlier files spelled it).
    Gguf {
        model: FileName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mmproj: Option<FileName>,
    },
    /// A directory of ONNX files and `tokens.txt`, as the speech host reads it.
    SherpaDir,
}

/// The name a server answers to in a request's `model` field (`--served-model-name`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServedName(pub String);

/// A server-side parser's name as the engine spells it (`qwen3_coder`, `qwen3`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParserName(pub String);

/// An engine somebody else started, on another machine, that the entry is reached through. The
/// entry carries no `[[engine]]` profile and no GPU share: nothing here spawns it or budgets for
/// it. It names what a client needs: the wire (`engine`), the name to ask for and the parsers the
/// server runs. The measured server command is the entry's comment block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachedEngine {
    pub engine: EngineKind,
    pub served_name: ServedName,
    pub tool_parser: ParserName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_parser: Option<ParserName>,
}

/// Who starts the entry's engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Serving {
    /// This computer starts it from the entry's `[[engine]]` profiles, within its GPU share.
    #[default]
    Launched,
    /// Already running elsewhere; see `AttachedEngine`.
    Attached(AttachedEngine),
}

impl Serving {
    pub fn is_launched(&self) -> bool {
        matches!(self, Serving::Launched)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineProfile {
    pub kind: EngineKind,
    pub args: Vec<EngineArg>,
    pub weights: WeightFiles,
    /// The inputs of the model this engine passes through, when it passes fewer than the model
    /// takes (an engine build with no audio path). `None` passes all of them. Never wider than
    /// the model's own inputs; `parse_entry` checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Modalities>,
    /// The same for outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Modalities>,
}
