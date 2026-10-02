//! Request encoding and stream decoding, with no I/O: the chat-completions codec.

use model_http::{Exchange, ResponseHead};
use model_provider::{
    EmbedEnd, EmbedTurn, ModelInfo, ModelName, ProviderError, TurnEnd, TurnEvent, TurnRequest,
};
use model_wire::{ChatCodec, ChatDecoder, CodecError, EmbedCodec, ErrorWire};
use serde::{Deserialize, Serialize};

/// Which server dialect of the shared wire to speak: they differ in where reasoning text arrives,
/// which constraint fields exist, and the usage chunk (see [`Flavor::quirks`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flavor {
    LlamaServer,
    Vllm,
    LiteLlm,
    OpenRouter,
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
    todo!(
        "encode_request: messages, images, tools, constraints, sampling, engine extras, stream_options"
    )
}

/// The OpenAI-compatible codec of one flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiCodec {
    flavor: Flavor,
}

impl OpenAiCodec {
    pub fn new(flavor: Flavor) -> Self {
        Self { flavor }
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    /// This codec over a client: the provider an engine is spoken to through.
    pub fn provider(self, client: model_http::HttpClient) -> crate::OpenAiCompat {
        model_wire::Driver::new(self, client)
    }
}

impl ErrorWire for OpenAiCodec {
    fn classify(&self, head: &ResponseHead, body: &[u8]) -> ProviderError {
        let _ = (self.flavor, head, body);
        todo!("OpenAiCodec::classify: status classes, the {{\"error\": ...}} envelope, an HTML 200")
    }
}

impl ChatCodec for OpenAiCodec {
    type Decoder = StreamDecoder;

    fn encode(&self, request: &TurnRequest) -> Result<Exchange, CodecError> {
        let _ = (self.flavor, request);
        todo!("OpenAiCodec::encode: POST /chat/completions, Framing::Sse")
    }

    fn decoder(&self, served: ModelName) -> StreamDecoder {
        StreamDecoder::new(self.flavor, served)
    }

    fn describe(&self) -> Exchange {
        let _ = self.flavor;
        todo!("OpenAiCodec::describe: GET /models, or /props at the server root for llama-server")
    }

    fn parse_models(&self, body: &[u8]) -> Result<Vec<ModelInfo>, CodecError> {
        let _ = (self.flavor, body);
        todo!("OpenAiCodec::parse_models: loaded and trained context per flavor")
    }
}

impl EmbedCodec for OpenAiCodec {
    fn encode_embed(&self, turn: &EmbedTurn) -> Result<Exchange, CodecError> {
        let _ = (self.flavor, turn);
        todo!(
            "OpenAiCodec::encode_embed: POST /embeddings, no dimensions where the quirk says Ignored"
        )
    }

    fn decode_embed(&self, served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError> {
        let _ = (self.flavor, served, body);
        todo!("OpenAiCodec::decode_embed: data[].embedding in index order")
    }
}

/// Turns SSE data frames into turn events. Tool-call argument fragments are joined per index and
/// checked as JSON; `[DONE]`, or a finish reason followed by the usage chunk, ends the stream; a
/// malformed frame is `Unreadable`, never a panic.
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    flavor: Flavor,
    served: ModelName,
}

impl StreamDecoder {
    pub fn new(flavor: Flavor, served: ModelName) -> Self {
        Self { flavor, served }
    }
}

impl ChatDecoder for StreamDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError> {
        let _ = (&self.flavor, frame);
        todo!("StreamDecoder::feed: deltas, reasoning, tool-call fragments, usage")
    }

    fn finish(self) -> Result<TurnEnd, CodecError> {
        let _ = self.served;
        todo!("StreamDecoder::finish: stop reason and usage")
    }
}
