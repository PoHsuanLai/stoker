//! Speech cassettes: the same JSON Lines file as the chat ones, with speech interactions.
//!
//! A request is stored as a fingerprint: audio becomes a digest, a length and a duration, text
//! to speak becomes a digest and a length. Audio is never in the file. What a recognised
//! transcript says is: a fixture's audio is synthetic or recorded by a dev script by hand, so
//! its transcript is the fixture's expected output. A replayed synthesis answers silence of the
//! recorded duration.

use model_provider::{ModelName, ProviderError};
use serde::{Deserialize, Serialize};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, AudioSink, AudioSource, Lang, LangChoice, SpeechModelInfo,
    SpeechToText, SttEnd, SttMode, SttRequest, TextToSpeech, TranscriptEvent, TranscriptSink,
    TtsEnd, TtsRequest, VoiceId,
};

use crate::cassette::{read_jsonl, write_jsonl};
use crate::{ByteCount, Cassette, CassetteError, CassetteHeader, ReplayMode, SinkError};

/// A BLAKE3 digest of audio samples, 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudioDigest(pub String);

/// A BLAKE3 digest of text, 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextDigest(pub String);

/// Audio as a cassette keeps it: format, digest, size and length, never the samples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPrint {
    pub format: AudioFormat,
    pub digest: AudioDigest,
    pub len: ByteCount,
    pub duration: AudioMs,
}

impl AudioPrint {
    /// The print of `chunks`, hashed in order.
    pub fn of(chunks: &[AudioChunk]) -> AudioPrint {
        let _ = chunks;
        todo!("AudioPrint::of: blake3 over the samples, summed length and duration")
    }
}

/// Text as a cassette keeps it: a digest and a length.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPrint {
    pub digest: TextDigest,
    pub len: ByteCount,
}

/// An `SttRequest` and the audio it was given, as a fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SttPrint {
    pub model: ModelName,
    pub mode: SttMode,
    pub lang: LangChoice,
    pub format: AudioFormat,
    pub audio: AudioPrint,
}

impl SttPrint {
    pub fn of(request: &SttRequest, audio: &[AudioChunk]) -> SttPrint {
        let _ = (request, audio);
        todo!("SttPrint::of: the request's fields and AudioPrint::of the chunks")
    }
}

/// A `TtsRequest` as a fingerprint: the text is a digest and a length.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsPrint {
    pub model: ModelName,
    pub text: TextPrint,
    pub voice: VoiceId,
    pub lang: Lang,
    pub format: AudioFormat,
}

impl TtsPrint {
    pub fn of(request: &TtsRequest) -> TtsPrint {
        let _ = request;
        todo!("TtsPrint::of: blake3 over the text")
    }
}

/// One recorded call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SpeechInteraction {
    Stt {
        request: SttPrint,
        events: Vec<TranscriptEvent>,
        end: Result<SttEnd, ProviderError>,
    },
    Tts {
        request: TtsPrint,
        /// What the engine made; a replay plays silence of this duration.
        audio: AudioPrint,
        end: Result<TtsEnd, ProviderError>,
    },
}

/// `<name>.speech.cassette.jsonl`: the chat cassette's header, then one line per interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechCassette {
    pub header: CassetteHeader,
    pub interactions: Vec<SpeechInteraction>,
}

impl SpeechCassette {
    pub fn to_jsonl(&self) -> String {
        write_jsonl(&self.header, &self.interactions)
    }

    pub fn from_jsonl(text: &str) -> Result<SpeechCassette, CassetteError> {
        let (header, interactions) = read_jsonl(text)?;
        Ok(SpeechCassette {
            header,
            interactions,
        })
    }
}

impl From<Cassette> for SpeechCassette {
    /// A chat cassette's header with no speech interactions, to start a new file.
    fn from(chat: Cassette) -> Self {
        SpeechCassette {
            header: chat.header,
            interactions: Vec::new(),
        }
    }
}

/// Plays a speech cassette as both providers.
#[derive(Debug)]
pub struct SpeechReplay {
    cassette: SpeechCassette,
    mode: ReplayMode,
}

impl SpeechReplay {
    pub fn new(cassette: SpeechCassette, mode: ReplayMode) -> Self {
        Self { cassette, mode }
    }
}

impl SpeechToText for SpeechReplay {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let _ = &self.cassette.header;
        async { todo!("SpeechReplay::describe: the cassette's model") }
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        let _ = (&self.cassette, &self.mode, request, &mut *audio, &mut *sink);
        async {
            todo!("SpeechReplay::transcribe: drain the audio, match, push events, return the end")
        }
    }
}

impl TextToSpeech for SpeechReplay {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        let _ = &self.cassette.header;
        async { todo!("SpeechReplay::describe: the cassette's model") }
    }

    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send {
        let _ = (&self.cassette, &self.mode, request, &mut *sink);
        async { todo!("SpeechReplay::speak: match, push silent chunks of the recorded duration") }
    }
}

/// A sink for recorded speech interactions, injected so the crate itself writes no files.
pub trait SpeechCassetteSink: Send + Sync {
    fn write(&self, line: &SpeechInteraction) -> Result<(), SinkError>;
}

/// Wraps a speech provider and records every call into a sink.
#[derive(Debug)]
pub struct RecordingSpeech<P, C: SpeechCassetteSink> {
    inner: P,
    sink: C,
}

impl<P, C: SpeechCassetteSink> RecordingSpeech<P, C> {
    pub fn new(inner: P, sink: C) -> Self {
        Self { inner, sink }
    }
}

impl<P: SpeechToText, C: SpeechCassetteSink> SpeechToText for RecordingSpeech<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        let _ = (&self.inner, &self.sink, request, &mut *audio, &mut *sink);
        async { todo!("RecordingSpeech::transcribe: tee audio and events into an interaction") }
    }
}

impl<P: TextToSpeech, C: SpeechCassetteSink> TextToSpeech for RecordingSpeech<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send {
        let _ = (&self.inner, &self.sink, request, &mut *sink);
        async { todo!("RecordingSpeech::speak: tee the audio into a print, write the interaction") }
    }
}
