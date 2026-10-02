//! The client of `speech-host`, the STT engine process, over `$XDG_RUNTIME_DIR/inferd/<engine>.sock`.
//!
//! It speaks `speech_provider::host_wire` (a 4-byte length, then JSON). `SpeechHostClient` is
//! one of the `SttBackend` arms in porter's inferd. The socket I/O joins with `tokio` (pinned
//! block, `net` and `io-util`) when the body is filled.

use std::path::PathBuf;

use model_provider::ProviderError;
use serde::{Deserialize, Serialize};
use speech_provider::{
    AudioSource, SpeechModelInfo, SpeechToText, SttEnd, SttRequest, TranscriptSink,
};

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
        let _ = &self.socket;
        async { todo!("SpeechHostClient::describe: Hello, read the models") }
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        let _ = (&self.socket, request, &mut *audio, &mut *sink);
        async {
            todo!(
                "SpeechHostClient::transcribe: Begin, pump audio and events, End or Cancel on drop"
            )
        }
    }
}
