//! The fingerprint of a request: what a cassette stores in place of the request itself.

use model_provider::{
    ImageDetail, Limits, ModelName, OutputShape, Reasoning, Role, ToolCall, ToolCallId, ToolChoice,
    ToolSpec, ToolStatus, TurnRequest,
};
use serde::{Deserialize, Serialize};
use vision_prep::MediaType;

/// A length in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteCount(pub u64);

/// A BLAKE3 digest, written as 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageDigest(pub String);

/// An image as a cassette keeps it: a digest and a size, never the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePrint {
    pub media: MediaType,
    pub digest: ImageDigest,
    pub len: ByteCount,
    pub detail: ImageDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum PartPrint {
    Text(String),
    Image(ImagePrint),
    Thought(String),
    ToolCall(ToolCall),
    ToolResult(ToolResultPrint),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultPrint {
    pub id: ToolCallId,
    pub status: ToolStatus,
    pub parts: Vec<PartPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePrint {
    pub role: Role,
    pub parts: Vec<PartPrint>,
}

/// A `TurnRequest` with its image bytes replaced by digests. No auth, no headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestPrint {
    pub model: ModelName,
    pub messages: Vec<MessagePrint>,
    pub tools: Vec<ToolSpec>,
    pub tool_choice: ToolChoice,
    pub output: OutputShape,
    pub limits: Limits,
    pub reasoning: Reasoning,
}

impl RequestPrint {
    /// The fingerprint of `request`: images become digest and size.
    pub fn of(request: &TurnRequest) -> RequestPrint {
        let _ = request;
        todo!("RequestPrint::of: blake3 over image bytes")
    }
}
