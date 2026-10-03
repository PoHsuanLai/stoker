//! What a parse returns.

use cua_action::{CuaAction, GridMax, GridSpace, ImageSpace};
use serde::{Deserialize, Serialize};

use crate::ByteOffset;

/// Actions in the space the dialect speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum InSpace {
    Image(Vec<CuaAction<ImageSpace>>),
    Grid(GridMax, Vec<CuaAction<GridSpace>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parsed {
    pub thought: Option<String>,
    pub actions: InSpace,
    pub dropped: Vec<Dropped>,
}

/// A verb the model wrote that did not become an action, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    pub verb: VerbText,
    pub reason: DropReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    UnsupportedVerb,
    MissingArgument,
    /// An argument is there but unusable: empty or control-character text, a key name that is no
    /// key, a direction that is no direction.
    BadArgument,
    BadNumber,
    TooLong,
    OverBatchLimit,
    /// A point mapped outside the frame; refused, never clamped.
    OutOfFrame,
}

/// A verb longer than 64 characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a verb is at most 64 characters")]
pub struct VerbTextError;

/// A verb as the model wrote it, cut to a size that is safe to log and show: at most 64
/// characters, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VerbText(String);

impl VerbText {
    pub const MAX_CHARS: usize = 64;

    pub fn new(verb: impl Into<String>) -> Result<Self, VerbTextError> {
        let verb = verb.into();
        if verb.chars().count() <= Self::MAX_CHARS && !verb.chars().any(char::is_control) {
            Ok(Self(verb))
        } else {
            Err(VerbTextError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The verb as the model wrote it, cut to what a `VerbText` holds: control characters
    /// removed, at most [`VerbText::MAX_CHARS`] characters. Total, for reporting what a parser
    /// dropped.
    pub(crate) fn lossy(verb: &str) -> Self {
        VerbText(
            verb.chars()
                .filter(|c| !c.is_control())
                .take(Self::MAX_CHARS)
                .collect(),
        )
    }
}

impl TryFrom<String> for VerbText {
    type Error = VerbTextError;
    fn try_from(verb: String) -> Result<Self, VerbTextError> {
        VerbText::new(verb)
    }
}

impl From<VerbText> for String {
    fn from(verb: VerbText) -> String {
        verb.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ParseError {
    #[error("the reply holds no action")]
    NoAction,
    #[error("the reply ends inside an action")]
    Unterminated,
    #[error("malformed at byte {at:?}")]
    Malformed { at: ByteOffset },
    #[error("the reply is larger than the limit")]
    TooLarge,
}
