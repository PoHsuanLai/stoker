//! Credentials for a request, kept out of logs.

use core::fmt;

use serde::{Deserialize, Serialize};

/// A secret string. `Debug` prints nothing of it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeaderName(pub String);

/// The header that authenticates a request, if any. Local engines use `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthHeader {
    None,
    /// `Authorization: Bearer <secret>`.
    Bearer(Secret),
    /// A vendor's own header (`x-api-key`).
    Header {
        name: HeaderName,
        value: Secret,
    },
}

/// A header sent on every request beside the authenticating one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraHeader {
    pub name: HeaderName,
    pub value: Secret,
}
