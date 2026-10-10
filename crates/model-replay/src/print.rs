//! The fingerprint of a request: what a cassette stores in place of the request itself.

use model_provider::{
    EngineExtras, ImageDetail, Limits, ModelName, OutputShape, Part, Reasoning, Role, Sampling,
    ThoughtSeal, ToolCall, ToolCallId, ToolChoice, ToolParallelism, ToolSpec, ToolStatus,
    TurnRequest,
};
use serde::{Deserialize, Serialize};
use vision_prep::MediaType;

use crate::canon::{canonical, digest_hex, len64};

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
    Thought {
        text: String,
        seal: ThoughtSeal,
    },
    ToolCall(ToolCall),
    ToolResult(ToolResultPrint),
    /// A part kind this build does not know.
    Unknown,
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
    pub tool_calls: ToolParallelism,
    pub output: OutputShape,
    pub limits: Limits,
    pub sampling: Sampling,
    pub reasoning: Reasoning,
    pub engine: EngineExtras,
}

/// A BLAKE3 digest of the canonical JSON (keys sorted) of a [`RequestPrint`], 64 lowercase hex
/// characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrintHash(pub String);

impl RequestPrint {
    /// The fingerprint of `request`: images become digest and size.
    pub fn of(request: &TurnRequest) -> RequestPrint {
        RequestPrint {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| MessagePrint {
                    role: m.role,
                    parts: m.parts.iter().map(part_print).collect(),
                })
                .collect(),
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            tool_calls: request.tool_calls,
            output: request.output.clone(),
            limits: request.limits.clone(),
            sampling: request.sampling,
            reasoning: request.reasoning,
            engine: request.engine,
        }
    }

    /// The hash of this print's canonical JSON.
    pub fn hash(&self) -> PrintHash {
        // Serialising these types cannot fail: no map has non-string keys.
        let value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        PrintHash(digest_hex(canonical(&value).as_bytes()))
    }
}

fn part_print(part: &Part) -> PartPrint {
    match part {
        Part::Text(text) => PartPrint::Text(text.clone()),
        Part::Image(image) => PartPrint::Image(ImagePrint {
            media: image.media,
            digest: ImageDigest(digest_hex(&image.bytes.0)),
            len: ByteCount(len64(image.bytes.0.len())),
            detail: image.detail,
        }),
        Part::Thought { text, seal } => PartPrint::Thought {
            text: text.clone(),
            seal: seal.clone(),
        },
        Part::ToolCall(call) => PartPrint::ToolCall(call.clone()),
        Part::ToolResult(result) => PartPrint::ToolResult(ToolResultPrint {
            id: result.id.clone(),
            status: result.status,
            parts: result.parts.iter().map(part_print).collect(),
        }),
        _ => PartPrint::Unknown,
    }
}
