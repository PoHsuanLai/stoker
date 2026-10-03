//! Request encoding and stream decoding, with no I/O: the chat-completions codec.

use model_http::{Exchange, Framing, JsonBody, ResponseHead, RouteRoot, UrlPath, Verb};
use model_provider::{EmbedEnd, EmbedTurn, ModelInfo, ModelName, ProviderError, TurnRequest};
use model_wire::{ChatCodec, CodecError, EmbedCodec, ErrorWire};
use serde::{Deserialize, Serialize};

use crate::classify::classify;
use crate::request::encode_request;
use crate::{StreamDecoder, embed, models};

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
        classify(head, body)
    }
}

impl ChatCodec for OpenAiCodec {
    type Decoder = StreamDecoder;

    fn encode(&self, request: &TurnRequest) -> Result<Exchange, CodecError> {
        let body = encode_request(request, self.flavor)?;
        Ok(Exchange {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/chat/completions".into()),
            body: Some(JsonBody(body.0)),
            framing: Framing::Sse,
        })
    }

    fn decoder(&self, served: ModelName) -> StreamDecoder {
        StreamDecoder::new(self.flavor, served)
    }

    fn describe(&self) -> Exchange {
        models::describe(self.flavor)
    }

    fn parse_models(&self, body: &[u8]) -> Result<Vec<ModelInfo>, CodecError> {
        models::parse_models(body)
    }
}

impl EmbedCodec for OpenAiCodec {
    fn encode_embed(&self, turn: &EmbedTurn) -> Result<Exchange, CodecError> {
        Ok(embed::encode(self.flavor, turn))
    }

    fn decode_embed(&self, served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError> {
        embed::decode(served, body)
    }
}
