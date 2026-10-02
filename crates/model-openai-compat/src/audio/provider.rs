//! The speech provider over `model-http`.

use model_http::HttpClient;
use model_provider::ProviderError;
use speech_provider::{
    AudioSink, AudioSource, SpeechModelInfo, SpeechToText, SttEnd, SttRequest, TextToSpeech,
    TranscriptSink, TtsEnd, TtsRequest,
};

use super::SpeechFlavor;

/// One OpenAI-compatible speech endpoint. The same value serves `SpeechToText` where the server
/// has `/audio/transcriptions` and `TextToSpeech` where it has `/audio/speech`; a call to the
/// other direction answers `ProviderError::BadRequest`.
#[derive(Debug, Clone)]
pub struct OpenAiSpeech {
    client: HttpClient,
    flavor: SpeechFlavor,
}

impl OpenAiSpeech {
    pub fn new(client: HttpClient, flavor: SpeechFlavor) -> Self {
        Self { client, flavor }
    }

    pub fn flavor(&self) -> SpeechFlavor {
        self.flavor
    }
}

impl SpeechToText for OpenAiSpeech {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let _ = &self.client;
        async { todo!("OpenAiSpeech::describe (in): GET /models, caps from the catalog entry") }
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        let _ = (&self.client, request, &mut *audio, &mut *sink);
        async {
            todo!("OpenAiSpeech::transcribe: drain the audio, POST multipart, one Final event")
        }
    }
}

impl TextToSpeech for OpenAiSpeech {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let _ = &self.client;
        async { todo!("OpenAiSpeech::describe (out): GET /models and /audio/voices") }
    }

    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send {
        let _ = (&self.client, request, &mut *sink);
        async { todo!("OpenAiSpeech::speak: POST /audio/speech, PcmDecoder into the sink") }
    }
}
