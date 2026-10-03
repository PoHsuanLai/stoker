//! The seams a wire implements.

use model_http::{Exchange, ResponseHead};
use model_provider::{
    EmbedEnd, EmbedTurn, ModelInfo, ModelName, ProviderError, TurnEnd, TurnEvent, TurnRequest,
};

/// Why a request cannot be encoded or a reply cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("this wire cannot enforce the requested output shape")]
    UnsupportedShape,
    #[error("a frame is not a chunk of this wire")]
    Unreadable,
    #[error("the stream ended before a finish reason")]
    Truncated,
    #[error("tool-call arguments are not valid JSON")]
    BadToolArguments,
}

/// Reading an error response. Per wire, because each puts its error in its own envelope.
pub trait ErrorWire {
    /// Maps a non-success response, or an HTML page served as 200, to the error a caller acts on.
    /// `body` is at most the first 64 KiB. The result never carries the body (it can echo the
    /// prompt).
    fn classify(&self, head: &ResponseHead, body: &[u8]) -> ProviderError;
}

/// The request and reply format of a chat endpoint.
pub trait ChatCodec: ErrorWire + Send + Sync {
    type Decoder: ChatDecoder + Send;

    fn encode(&self, request: &TurnRequest) -> Result<Exchange, CodecError>;

    /// A fresh decoder for one reply; `served` is the model name to report in `TurnEnd`.
    fn decoder(&self, served: ModelName) -> Self::Decoder;

    /// The request that lists models (`GET /models`, `/props`).
    fn describe(&self) -> Exchange;

    fn parse_models(&self, body: &[u8]) -> Result<Vec<ModelInfo>, CodecError>;
}

/// Reads one reply, frame by frame. A frame is one SSE event's data, one NDJSON line, or the whole
/// body, as the exchange's `Framing` said.
pub trait ChatDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError>;

    /// The end of the turn, once the stream is over.
    fn finish(self) -> Result<TurnEnd, CodecError>;

    /// The error a frame carried, after `feed` answered `Unreadable` for an error envelope
    /// delivered inside a 200 stream: what the driver reports instead of a generic unreadable
    /// reply. A wire with no such envelope keeps the default.
    fn fault(&self) -> Option<ProviderError> {
        None
    }
}

/// The request and reply format of an embeddings endpoint.
pub trait EmbedCodec: ErrorWire + Send + Sync {
    /// `POST /embeddings` (or the engine's own route) with the role's prefix already applied.
    fn encode_embed(&self, turn: &EmbedTurn) -> Result<Exchange, CodecError>;

    /// Reads the whole reply body.
    fn decode_embed(&self, served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError>;
}
