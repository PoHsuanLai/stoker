//! Request encoding and stream decoding, with no I/O.

use model_http::SseEvent;
use model_provider::{ModelName, TurnEnd, TurnEvent, TurnRequest};
use serde::{Deserialize, Serialize};

/// Which server dialect of the shared wire to speak: they differ in where reasoning text arrives,
/// which constraint fields exist, and the usage chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flavor {
    LlamaServer,
    Vllm,
    LiteLlm,
    OpenRouter,
}

/// Why a request cannot be encoded or a stream cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("this flavor cannot enforce the requested output shape")]
    UnsupportedShape,
    #[error("a stream line is not a chat-completions chunk")]
    Unreadable,
    #[error("the stream ended before a finish reason")]
    Truncated,
    #[error("tool-call arguments are not valid JSON")]
    BadToolArguments,
}

/// A request body, as JSON text.
#[derive(Clone, PartialEq, Eq)]
pub struct RequestJson(pub String);

// A request carries the prompt: Debug shows the length only.
impl core::fmt::Debug for RequestJson {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RequestJson(<{} bytes>)", self.0.len())
    }
}

/// The JSON body of `POST /chat/completions` for `request`, streaming on.
pub fn encode_request(request: &TurnRequest, flavor: Flavor) -> Result<RequestJson, CodecError> {
    let _ = (request, flavor);
    todo!("encode_request: messages, images, tools, constraints, stream_options")
}

/// Turns SSE events into turn events. Tool-call argument fragments are joined per index and
/// checked as JSON; `[DONE]`, or a finish reason followed by the usage chunk, ends the stream; a
/// malformed line is `Unreadable`, never a panic.
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    flavor: Flavor,
    served: ModelName,
}

impl StreamDecoder {
    pub fn new(flavor: Flavor, served: ModelName) -> Self {
        Self { flavor, served }
    }

    pub fn feed(&mut self, event: &SseEvent) -> Result<Vec<TurnEvent>, CodecError> {
        let _ = (&self.flavor, event);
        todo!("StreamDecoder::feed: deltas, reasoning, tool-call fragments, usage")
    }

    /// The end of the turn, once the stream is over.
    pub fn finish(self) -> Result<TurnEnd, CodecError> {
        let _ = self.served;
        todo!("StreamDecoder::finish: stop reason and usage")
    }
}
