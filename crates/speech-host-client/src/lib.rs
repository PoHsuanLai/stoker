//! The client of `speech-host`, the STT engine process, over `$XDG_RUNTIME_DIR/inferd/<engine>.sock`.
//!
//! It speaks `speech_provider::host_wire` (a 4-byte length, then JSON). `SpeechHostClient` is
//! one of the `SttBackend` arms in porter's inferd.
//!
//! Errors: a host that cannot be reached, closes early or breaks the stream is `Unreachable`; a
//! frame that is over the cap, is not JSON of the vocabulary, or a host of another vocabulary
//! version is `Unreadable`; a `Failed` frame is passed through as the error it carries.
//!
//! The `net` feature (on by default) holds everything that needs tokio: the `SpeechToText`
//! impl and the socket framing. Without it the crate keeps `HostSocket`, `SpeechHostClient` and
//! `FrameBuffer`, so a consumer can name the types without a runtime.

#[cfg(feature = "net")]
mod client;
mod framed;
#[cfg(feature = "net")]
mod session;
#[cfg(feature = "net")]
mod stream;

pub use framed::FrameBuffer;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The host's Unix socket.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostSocket(pub PathBuf);

/// One host. A connection is made per utterance; cancellation is drop, which closes it.
#[derive(Debug, Clone)]
pub struct SpeechHostClient {
    socket: HostSocket,
}

impl SpeechHostClient {
    pub fn new(socket: HostSocket) -> Self {
        Self { socket }
    }

    pub fn socket(&self) -> &HostSocket {
        &self.socket
    }
}
