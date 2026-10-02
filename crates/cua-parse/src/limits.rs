//! Bounds on what a parser reads and returns.

use serde::{Deserialize, Serialize};

/// A length in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteLen(pub u32);

/// A position in the input, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteOffset(pub u32);

/// A number of actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionCount(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseLimits {
    pub max_input: ByteLen,
    pub max_actions: ActionCount,
}

impl Default for ParseLimits {
    /// 32 KiB of input, 8 actions.
    fn default() -> Self {
        ParseLimits {
            max_input: ByteLen(32 * 1024),
            max_actions: ActionCount(8),
        }
    }
}
