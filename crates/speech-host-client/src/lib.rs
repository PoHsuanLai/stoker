//! The client of `speech-host`, the STT engine process, over `$XDG_RUNTIME_DIR/inferd/<engine>.sock`.
//!
//! It speaks `speech_provider::host_wire` (a 4-byte length, then JSON). `SpeechHostClient` is
//! one of the `SttBackend` arms in porter's inferd.
//!
//! Errors: a host that cannot be reached, closes early or breaks the stream is `Unreachable`; a
//! frame that is over the cap, is not JSON of the vocabulary, or a host of another vocabulary
//! version is `Unreadable`; a `Failed` frame is passed through as the error it carries.

mod framed;
mod session;

pub use framed::FrameBuffer;

use std::path::PathBuf;

use model_provider::ProviderError;
use serde::{Deserialize, Serialize};
use speech_provider::{
    AudioSource, SpeechModelInfo, SpeechToText, SttEnd, SttRequest, TranscriptSink,
};
use tokio::net::UnixStream;

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

impl SpeechToText for SpeechHostClient {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let socket = self.socket.clone();
        async move {
            let stream = connect(&socket).await?;
            let mut guard = session::Wire::new(&stream);
            let mut reader = framed::FrameReader::default();
            let models = session::hello(&mut guard, &mut reader).await?;
            guard.finish();
            Ok(models)
        }
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        let socket = self.socket.clone();
        let request = request.clone();
        async move {
            let stream = connect(&socket).await?;
            session::utterance(&stream, request, audio, sink).await
        }
    }
}

async fn connect(socket: &HostSocket) -> Result<UnixStream, ProviderError> {
    UnixStream::connect(&socket.0)
        .await
        .map_err(|_| ProviderError::Unreachable)
}
