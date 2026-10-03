//! The speech provider over `model-http`.

use model_http::{
    ContentType, Exchange, Framing, HttpClient, JsonBody, RawBody, RouteRoot, Upload,
    UploadTransport, UrlPath, Verb,
};
use model_provider::{ModelName, ProviderError};
use speech_provider::{
    AudioChunk, AudioMs, AudioPull, AudioSink, AudioSource, SampleIndex, SpeechCaps, SpeechIo,
    SpeechModelInfo, SpeechToText, SttEnd, SttRequest, TextToSpeech, TranscriptEvent,
    TranscriptSink, TtsEnd, TtsRequest,
};

use super::encode_transcription;
use super::reply::{Pcm, Whole, listed_models, listed_voices};
use super::{AudioCodecError, SpeechFlavor, decode_transcription, encode_speech_request};

/// The most audio one transcription takes in, as bytes of PCM (about 35 minutes of 16 kHz S16).
const UPLOAD_PCM_MAX: usize = 64 << 20;

/// One OpenAI-compatible speech endpoint. The same value serves `SpeechToText` where the server
/// has `/audio/transcriptions` and `TextToSpeech` where it has `/audio/speech`; a call to the
/// other direction answers `ProviderError::BadRequest`.
///
/// The transport is [`HttpClient`] unless a test supplies another.
#[derive(Debug, Clone)]
pub struct OpenAiSpeech<T = HttpClient> {
    client: T,
    flavor: SpeechFlavor,
    known: Vec<SpeechModelInfo>,
}

impl<T> OpenAiSpeech<T> {
    pub fn new(client: T, flavor: SpeechFlavor) -> Self {
        Self {
            client,
            flavor,
            known: Vec::new(),
        }
    }

    /// The same provider that knows what these models can do, named as the server serves them
    /// (the catalog's `speech` table gives the caps). `describe` answers the ones the server
    /// lists; a served model the catalog does not know is left out, because its caps cannot be
    /// said.
    pub fn with_known(self, known: Vec<SpeechModelInfo>) -> Self {
        Self { known, ..self }
    }

    pub fn flavor(&self) -> SpeechFlavor {
        self.flavor
    }

    fn no_endpoint(what: &str) -> ProviderError {
        ProviderError::BadRequest(format!("this server has no {what} endpoint"))
    }
}

fn get(path: &str) -> Exchange {
    Exchange {
        verb: Verb::Get,
        root: RouteRoot::Base,
        path: UrlPath(path.into()),
        body: None,
        framing: Framing::Whole,
    }
}

fn codec_error(error: AudioCodecError) -> ProviderError {
    ProviderError::BadRequest(match error {
        AudioCodecError::UnsupportedFormat => "this server cannot take the audio format".into(),
        AudioCodecError::MixedFormats => "the audio chunks do not share one format".into(),
        AudioCodecError::Unreadable => "the response is not the documented JSON".into(),
    })
}

/// The models the server lists that `known` has caps for, the voices of the out direction read
/// from the server where it lists any.
fn described(
    known: &[SpeechModelInfo],
    listed: &[ModelName],
    voices: Option<&[speech_provider::VoiceId]>,
) -> Vec<SpeechModelInfo> {
    known
        .iter()
        .filter(|info| listed.contains(&info.name))
        .map(|info| match (&info.caps.io, voices) {
            (SpeechIo::Out { output, .. }, Some(voices)) if !voices.is_empty() => SpeechModelInfo {
                name: info.name.clone(),
                caps: SpeechCaps {
                    io: SpeechIo::Out {
                        output: *output,
                        voices: voices.to_vec(),
                    },
                    ..info.caps.clone()
                },
            },
            _ => info.clone(),
        })
        .collect()
}

/// The first sample of the audio and one past the last.
fn span(audio: &[AudioChunk]) -> (SampleIndex, SampleIndex) {
    let from = audio.first().map_or(0, |c| c.at.0);
    let to = audio
        .last()
        .map_or(0, |c| c.at.0.saturating_add(u64::from(c.samples())));
    (SampleIndex(from), SampleIndex(to))
}

fn length(audio: &[AudioChunk]) -> AudioMs {
    AudioMs(
        audio
            .iter()
            .fold(0_u32, |total, c| total.saturating_add(c.duration().0)),
    )
}

/// Pulls the source dry, refusing more audio than one upload takes.
async fn drain<A: AudioSource>(audio: &mut A) -> Result<Vec<AudioChunk>, ProviderError> {
    let mut chunks = Vec::new();
    let mut bytes = 0_usize;
    while let AudioPull::Chunk(chunk) = audio.next().await {
        bytes = bytes.saturating_add(chunk.pcm.len());
        if bytes > UPLOAD_PCM_MAX {
            return Err(ProviderError::BadRequest(
                "the audio is longer than one upload takes".into(),
            ));
        }
        chunks.push(chunk);
    }
    Ok(chunks)
}

impl<T: UploadTransport> SpeechToText for OpenAiSpeech<T> {
    async fn describe(&self) -> Result<Vec<SpeechModelInfo>, ProviderError> {
        if self.flavor != SpeechFlavor::Vllm {
            return Err(Self::no_endpoint("transcription"));
        }
        let mut models = Whole::default();
        let sent = self.client.exchange(&get("/models"), &mut models).await;
        let listed = listed_models(&models.into_body(sent)?)?;
        let known: Vec<SpeechModelInfo> = self
            .known
            .iter()
            .filter(|info| matches!(info.caps.io, SpeechIo::In { .. }))
            .cloned()
            .collect();
        Ok(described(&known, &listed, None))
    }

    async fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> Result<SttEnd, ProviderError> {
        if self.flavor != SpeechFlavor::Vllm {
            return Err(Self::no_endpoint("transcription"));
        }
        let chunks = drain(audio).await?;
        let (from, to) = span(&chunks);
        let heard = if chunks.is_empty() {
            // Nothing was said: there is nothing to send.
            speech_provider::HeardText(String::new())
        } else {
            let form = encode_transcription(request, &chunks).map_err(codec_error)?;
            let up = Upload {
                root: RouteRoot::Base,
                path: UrlPath("/audio/transcriptions".into()),
                body: RawBody {
                    content_type: ContentType(form.content_type),
                    bytes: form.bytes,
                },
                framing: Framing::Whole,
            };
            let mut reply = Whole::default();
            let sent = self.client.upload(&up, &mut reply).await;
            decode_transcription(&reply.into_body(sent)?).map_err(|_| {
                ProviderError::Unreadable("the transcription is not the documented JSON".into())
            })?
        };
        let audio = length(&chunks);
        // One final event: the endpoint is batch, so there is nothing to revise.
        let _ = sink.event(TranscriptEvent::Final {
            text: heard.clone(),
            from,
            to,
        });
        Ok(SttEnd {
            text: heard,
            audio,
            served: request.model.clone(),
        })
    }
}

impl<T: UploadTransport> TextToSpeech for OpenAiSpeech<T> {
    async fn describe(&self) -> Result<Vec<SpeechModelInfo>, ProviderError> {
        if self.flavor != SpeechFlavor::KokoroFastApi {
            return Err(Self::no_endpoint("speech"));
        }
        let mut models = Whole::default();
        let sent = self.client.exchange(&get("/models"), &mut models).await;
        let listed = listed_models(&models.into_body(sent)?)?;
        let mut voices = Whole::default();
        let sent = self
            .client
            .exchange(&get("/audio/voices"), &mut voices)
            .await;
        let voices = listed_voices(&voices.into_body(sent)?)?;
        let known: Vec<SpeechModelInfo> = self
            .known
            .iter()
            .filter(|info| matches!(info.caps.io, SpeechIo::Out { .. }))
            .cloned()
            .collect();
        Ok(described(&known, &listed, Some(&voices)))
    }

    async fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> Result<TtsEnd, ProviderError> {
        let body = encode_speech_request(request, self.flavor).map_err(codec_error)?;
        // The body streams in as the engine makes it: the transport hands each read over as it
        // arrives and `Whole` applies no framing, so the PCM decoder sees the raw bytes.
        let ex = Exchange {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/audio/speech".into()),
            body: Some(JsonBody(body.0)),
            framing: Framing::Whole,
        };
        let mut played = Pcm::new(request.format, sink);
        let sent = self.client.exchange(&ex, &mut played).await;
        Ok(TtsEnd {
            audio: played.into_played(sent)?,
            served: request.model.clone(),
        })
    }
}
