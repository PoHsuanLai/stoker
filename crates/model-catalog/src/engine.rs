//! How an engine is started for one model.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineProfile {
    pub kind: EngineKind,
    pub args: Vec<EngineArg>,
    pub weights: WeightFiles,
}
