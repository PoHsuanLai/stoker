//! The controls of a turn beyond its messages: sampling, tool parallelism, engine extras, and
//! the seal on a thought. All typed; there is no untyped JSON escape hatch (rig's
//! `additional_params`), so an engine's knob is a field here or it does not exist.

use serde::{Deserialize, Serialize};

use crate::{Count, Knob, Milli, OpaqueText, Seconds, Seed, SignatureText, SlotId, Tokens};

/// How a model samples its next token. Defaults per model come from the catalog entry
/// (`SamplingDefaults`); a request carries the values to use.
///
/// Thousandths throughout: `Milli(700)` is 0.7, `Milli(1100)` a repeat penalty of 1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sampling {
    pub temperature: Milli,
    pub top_p: Knob<Milli>,
    pub top_k: Knob<Count>,
    pub min_p: Knob<Milli>,
    pub repeat_penalty: Knob<Milli>,
    pub seed: Knob<Seed>,
}

/// Whether a turn may make several tool calls or only one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolParallelism {
    One,
    Many,
}

/// What only one engine flavor understands. A closed enum, one arm per flavor, so a knob for
/// vLLM cannot reach llama-server by accident; the codec for a flavor ignores the other arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineExtras {
    None,
    LlamaServer(LlamaExtras),
    Vllm(VllmExtras),
    Ollama(OllamaExtras),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LlamaExtras {
    pub cache_prompt: PromptCache,
    pub slot: Knob<SlotId>,
}

/// Whether the server reuses the KV cache of an unchanged prompt prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCache {
    Reuse,
    Fresh,
}

/// What a request tells vLLM beyond the shared fields. The constrained-decoding backend is not
/// among them: since vLLM 0.12 it is a serve-time flag (`--structured-outputs-config.backend`), so
/// it belongs to the engine's command line (`engine-supervisor`), not to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VllmExtras {
    /// The scheduling priority of this request (`priority`; lower runs first). It acts only
    /// when the engine was started with `--scheduling-policy priority`, which is how a
    /// foreground turn gets ahead of a background embedding rebuild.
    pub priority: Knob<Count>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OllamaExtras {
    pub keep_alive: KeepAlive,
    pub num_ctx: Knob<Tokens>,
}

/// How long Ollama keeps the model loaded after the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum KeepAlive {
    EngineDefault,
    Unload,
    Forever,
    For(Seconds),
}

/// What a provider attached to a thought so that it can be handed back unchanged on the next
/// turn. Local engines send none.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ThoughtSeal {
    None,
    /// The thought's text, with the provider's signature over it.
    Signed(SignatureText),
    /// The provider withheld the text; this is the encrypted block.
    Redacted(OpaqueText),
}
