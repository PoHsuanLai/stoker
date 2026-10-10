//! Names and checked text.

use core::fmt;

use serde::{Deserialize, Serialize};

/// An engine's served model name, or a vendor's model id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelName(pub String);

/// The id a model gives one tool call, echoed by its result. Opaque.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolCallId(pub String);

/// A tool name is not `[A-Za-z0-9_.-]{1,64}`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a tool name is 1 to 64 characters of [A-Za-z0-9_.-]")]
pub struct ToolNameError;

/// The name of a function a model may call.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ToolName(String);

impl ToolName {
    pub fn new(name: impl Into<String>) -> Result<Self, ToolNameError> {
        let name = name.into();
        let ok = (1..=64).contains(&name.len())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
        if ok {
            Ok(Self(name))
        } else {
            Err(ToolNameError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ToolName {
    type Error = ToolNameError;
    fn try_from(name: String) -> Result<Self, ToolNameError> {
        ToolName::new(name)
    }
}

impl From<ToolName> for String {
    fn from(name: ToolName) -> String {
        name.0
    }
}

/// The text is not JSON.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not valid JSON: {0}")]
pub struct JsonError(pub String);

/// Text that parses as JSON, checked where it enters. Model output is untrusted, so tool
/// arguments cross the boundary as `JsonText` and are read into a type by the code that owns it.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JsonText(String);

impl JsonText {
    pub fn new(text: impl Into<String>) -> Result<Self, JsonError> {
        let text = text.into();
        serde_json::from_str::<serde::de::IgnoredAny>(&text)
            .map_err(|e| JsonError(e.to_string()))?;
        Ok(Self(text))
    }

    /// The compact JSON of a `Value`. A `Value` always prints as valid JSON, so this cannot fail
    /// and needs no check.
    pub fn from_value(value: &serde_json::Value) -> Self {
        Self(value.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for JsonText {
    type Error = JsonError;
    fn try_from(text: String) -> Result<Self, JsonError> {
        JsonText::new(text)
    }
}

impl From<JsonText> for String {
    fn from(text: JsonText) -> String {
        text.0
    }
}

// Tool arguments can carry what the person typed: Debug shows the length only.
impl fmt::Debug for JsonText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JsonText(<{} bytes>)", self.0.len())
    }
}

/// A JSON Schema, as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaText(pub JsonText);

/// A model's signature over a thought, to be handed back unchanged (Anthropic `signature`,
/// OpenRouter `reasoning_details`). Opaque.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureText(pub String);

impl fmt::Debug for SignatureText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignatureText(<{} bytes>)", self.0.len())
    }
}

/// A thought the provider returned encrypted ("redacted thinking"), to be handed back unchanged.
/// Opaque.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueText(pub String);

impl fmt::Debug for OpaqueText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpaqueText(<{} bytes>)", self.0.len())
    }
}
