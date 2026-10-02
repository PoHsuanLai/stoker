//! The chat-completions wire that llama-server, vLLM, LiteLLM and OpenRouter share.
//!
//! `codec` is pure: a `TurnRequest` becomes request JSON, and the server's SSE events become
//! `TurnEvent`s. `OpenAiCompat` is the `Provider` that joins the codec to `model-http`.

mod codec;
mod provider;

pub use codec::{CodecError, Flavor, RequestJson, StreamDecoder, encode_request};
pub use provider::OpenAiCompat;
