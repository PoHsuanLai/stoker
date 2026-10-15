//! Credentials for a request, kept out of logs.
//!
//! Nothing here serializes: a key is read from where it is kept (a key file, the keyring) into a
//! [`Secret`], never written back out as part of an endpoint, a config echo or a cassette.

use core::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// A secret string. `Debug` prints nothing of it, it has no `Serialize`, and its text is wiped
/// when it drops.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HeaderName(pub String);

/// The header that authenticates a request, if any. Local engines use `None`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthHeader {
    None,
    /// `Authorization: Bearer <secret>`. Refused (`HttpError::Insecure`) on a plain `Tcp` target:
    /// a key goes over TLS or a Unix socket.
    Bearer(Secret),
    /// `Authorization: Bearer <secret>` to a `Tcp` target on this machine (`localhost`, `127.0.0.0/8`,
    /// `::1`): the explicit opt-in to send a key over plain http on the loopback interface. Any
    /// other plain `Tcp` host is still refused.
    LoopbackBearer(Secret),
    /// A vendor's own header (`x-api-key`).
    Header {
        name: HeaderName,
        value: Secret,
    },
}

/// A header sent on every request beside the authenticating one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExtraHeader {
    pub name: HeaderName,
    pub value: Secret,
}
