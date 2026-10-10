//! The `SpeechToText` impl over the host's Unix socket (the `net` feature).

use model_provider::ProviderError;
use speech_provider::{
    AudioSource, SpeechModelInfo, SpeechToText, SttEnd, SttRequest, TranscriptSink,
};
use tokio::net::UnixStream;

use crate::{HostSocket, SpeechHostClient, session};

impl SpeechToText for SpeechHostClient {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let socket = self.socket.clone();
        async move {
            let stream = connect(&socket).await?;
            let mut guard = session::Wire::new(&stream);
            let mut reader = crate::stream::FrameReader::default();
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
