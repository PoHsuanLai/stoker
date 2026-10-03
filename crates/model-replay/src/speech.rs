//! Speech cassettes: the same JSON Lines file as the chat ones, with speech interactions.
//!
//! A request is stored as a fingerprint: audio becomes a digest, a length and a duration, text
//! to speak becomes a digest and a length. Audio is never in the file. What a recognised
//! transcript says is: a fixture's audio is synthetic or recorded by a dev script by hand, so
//! its transcript is the fixture's expected output. A replayed synthesis answers silence of the
//! recorded duration.

use std::sync::Mutex;

use model_provider::{Flow, ModelName, ProviderError, Support};
use serde::{Deserialize, Serialize};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, AudioPull, AudioSink, AudioSource, HeardText, Lang,
    LangChoice, LangSet, PcmBytes, PcmFormat, SampleIndex, SampleRate, SpeechCaps, SpeechDir,
    SpeechIo, SpeechModelInfo, SpeechToText, SttEnd, SttMode, SttRequest, TextToSpeech,
    TranscriptEvent, TranscriptSink, TtsEnd, TtsRequest, VoiceId,
};

use crate::canon::{digest_hex, len64};
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

/// The format of an empty recording: speech-to-text input.
const FALLBACK_FORMAT: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

impl AudioPrint {
    /// The print of `chunks`, hashed in order.
    pub fn of(chunks: &[AudioChunk]) -> AudioPrint {
        let format = chunks.first().map_or(FALLBACK_FORMAT, |c| c.format);
        let bytes: Vec<u8> = chunks
            .iter()
            .flat_map(|c| c.pcm.as_slice())
            .copied()
            .collect();
        let samples: u64 = chunks.iter().map(|c| u64::from(c.samples())).sum();
        let ms = (samples * 1000)
            .checked_div(u64::from(format.rate.0))
            .unwrap_or(0);
        AudioPrint {
            format,
            digest: AudioDigest(digest_hex(&bytes)),
            len: ByteCount(len64(bytes.len())),
            duration: AudioMs(u32::try_from(ms).unwrap_or(u32::MAX)),
        }
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
        SttPrint {
            model: request.model.clone(),
            mode: request.mode,
            lang: request.lang.clone(),
            format: request.format,
            audio: AudioPrint::of(audio),
        }
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
        let text = request.text.as_str();
        TtsPrint {
            model: request.model.clone(),
            text: TextPrint {
                digest: TextDigest(digest_hex(text.as_bytes())),
                len: ByteCount(len64(text.len())),
            },
            voice: request.voice.clone(),
            lang: request.lang.clone(),
            format: request.format,
        }
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

/// Which interactions a speech replay has handed out. Speech-to-text and text-to-speech calls
/// are counted apart: the nth transcription gets the nth recorded transcription.
#[derive(Debug, Default)]
struct Cursor {
    stt_next: usize,
    tts_next: usize,
    stt_used: Vec<usize>,
    tts_used: Vec<usize>,
}

/// Plays a speech cassette as both providers.
///
/// Matching follows [`ReplayMode`], per direction, with the print compared whole (there is no
/// stored hash). A turn that finds nothing answers `NotReady` (nothing left) or `BadRequest`
/// (naming the interaction it was compared with).
#[derive(Debug)]
pub struct SpeechReplay {
    cassette: SpeechCassette,
    mode: ReplayMode,
    cursor: Mutex<Cursor>,
}

/// A recorded transcription, with where it sits in the cassette.
#[derive(Debug, Clone, Copy)]
struct SttCall<'a> {
    at: usize,
    print: &'a SttPrint,
    events: &'a [TranscriptEvent],
    end: &'a Result<SttEnd, ProviderError>,
}

/// A recorded synthesis, with where it sits in the cassette.
#[derive(Debug, Clone, Copy)]
struct TtsCall<'a> {
    at: usize,
    print: &'a TtsPrint,
    audio: &'a AudioPrint,
    end: &'a Result<TtsEnd, ProviderError>,
}

impl SpeechReplay {
    pub fn new(cassette: SpeechCassette, mode: ReplayMode) -> Self {
        Self {
            cassette,
            mode,
            cursor: Mutex::new(Cursor::default()),
        }
    }

    fn stt_calls(&self) -> Vec<SttCall<'_>> {
        self.cassette
            .interactions
            .iter()
            .enumerate()
            .filter_map(|(at, i)| match i {
                SpeechInteraction::Stt {
                    request,
                    events,
                    end,
                } => Some(SttCall {
                    at,
                    print: request,
                    events,
                    end,
                }),
                SpeechInteraction::Tts { .. } => None,
            })
            .collect()
    }

    fn tts_calls(&self) -> Vec<TtsCall<'_>> {
        self.cassette
            .interactions
            .iter()
            .enumerate()
            .filter_map(|(at, i)| match i {
                SpeechInteraction::Tts {
                    request,
                    audio,
                    end,
                } => Some(TtsCall {
                    at,
                    print: request,
                    audio,
                    end,
                }),
                SpeechInteraction::Stt { .. } => None,
            })
            .collect()
    }
}

/// The index (into `prints`) the next call gets, or why there is none.
fn choose<P: PartialEq>(
    prints: &[(usize, &P)],
    mode: ReplayMode,
    next: &mut usize,
    used: &mut Vec<usize>,
    got: &P,
) -> Result<usize, ProviderError> {
    let mismatch = |at: usize| {
        ProviderError::BadRequest(format!(
            "speech interaction {at} was recorded for a different request"
        ))
    };
    match mode {
        ReplayMode::InOrder | ReplayMode::Strict => {
            let (at, print) = *prints.get(*next).ok_or(ProviderError::NotReady)?;
            *next += 1;
            match mode {
                ReplayMode::Strict if *print != *got => Err(mismatch(at)),
                _ => Ok(at),
            }
        }
        ReplayMode::ByRequest => {
            let mut unused = prints.iter().filter(|(at, _)| !used.contains(at));
            let first = *unused.next().ok_or(ProviderError::NotReady)?;
            let at = std::iter::once(&first)
                .chain(unused)
                .find(|(_, p)| **p == *got)
                .map(|(at, _)| *at)
                .ok_or_else(|| mismatch(first.0))?;
            used.push(at);
            Ok(at)
        }
    }
}

/// Mono silence, `ms` long, in 20 ms chunks.
fn silence(format: AudioFormat, ms: AudioMs) -> Vec<AudioChunk> {
    let rate = u64::from(format.rate.0);
    let total = rate * u64::from(ms.0) / 1000;
    let step = (rate / 50).max(1);
    let width = u64::from(format.pcm.width());
    (0..total.div_ceil(step))
        .map(|n| {
            let at = n * step;
            let samples = step.min(total - at);
            AudioChunk {
                format,
                at: SampleIndex(at),
                pcm: PcmBytes::new(vec![0; usize::try_from(samples * width).unwrap_or(0)]),
            }
        })
        .collect()
}

fn caps(
    streaming: Support,
    partials: Support,
    timestamps: Support,
    max: AudioMs,
    io: SpeechIo,
) -> SpeechCaps {
    SpeechCaps {
        streaming,
        partials,
        punctuation: Support::Absent,
        timestamps,
        langs: LangSet::Any,
        max_audio: max,
        io,
    }
}

fn present(any: bool) -> Support {
    if any {
        Support::Present
    } else {
        Support::Absent
    }
}

impl SpeechReplay {
    /// The model the header describes, when it recorded capabilities for this direction.
    fn header_info(&self, dir: SpeechDir) -> Option<SpeechModelInfo> {
        let header = &self.cassette.header;
        header
            .speech
            .clone()
            .filter(|caps| caps.dir() == dir)
            .map(|caps| SpeechModelInfo {
                name: header.model.clone(),
                caps,
            })
    }

    /// The header's recorded capabilities when it is an input model's; otherwise what the
    /// recorded calls show: `Present` for streaming, partials and timestamps when some call used
    /// them, punctuation `Absent` (never recorded), any language, the longest audio seen, the
    /// first call's format. No such call, no model.
    fn stt_info(&self) -> Vec<SpeechModelInfo> {
        if let Some(info) = self.header_info(SpeechDir::In) {
            return vec![info];
        }
        let calls = self.stt_calls();
        let Some(first) = calls.first() else {
            return Vec::new();
        };
        let streaming = calls
            .iter()
            .any(|c| matches!(c.print.mode, SttMode::Streaming { .. }));
        let events = || calls.iter().flat_map(|c| c.events.iter());
        let partials = events().any(|e| matches!(e, TranscriptEvent::Partial { .. }));
        let finals = events().any(|e| matches!(e, TranscriptEvent::Final { .. }));
        let longest = calls
            .iter()
            .map(|c| c.print.audio.duration)
            .max()
            .unwrap_or_default();
        vec![SpeechModelInfo {
            name: self.cassette.header.model.clone(),
            caps: caps(
                present(streaming),
                present(partials),
                present(finals),
                longest,
                SpeechIo::In {
                    input: first.print.format,
                },
            ),
        }]
    }

    /// As [`Self::stt_info`], for synthesis: the voices recorded, the first call's output format.
    fn tts_info(&self) -> Vec<SpeechModelInfo> {
        if let Some(info) = self.header_info(SpeechDir::Out) {
            return vec![info];
        }
        let calls = self.tts_calls();
        let Some(first) = calls.first() else {
            return Vec::new();
        };
        let mut voices: Vec<VoiceId> = calls.iter().map(|c| c.print.voice.clone()).collect();
        voices.sort();
        voices.dedup();
        let longest = calls
            .iter()
            .map(|c| c.audio.duration)
            .max()
            .unwrap_or_default();
        vec![SpeechModelInfo {
            name: self.cassette.header.model.clone(),
            caps: caps(
                Support::Present,
                Support::Absent,
                Support::Absent,
                longest,
                SpeechIo::Out {
                    output: first.audio.format,
                    voices,
                },
            ),
        }]
    }
}

impl SpeechToText for SpeechReplay {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(self.stt_info()))
    }

    async fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> Result<SttEnd, ProviderError> {
        let mut heard = Vec::new();
        while let AudioPull::Chunk(chunk) = audio.next().await {
            heard.push(chunk);
        }
        let got = SttPrint::of(request, &heard);
        let calls = self.stt_calls();
        let prints: Vec<(usize, &SttPrint)> = calls.iter().map(|c| (c.at, c.print)).collect();
        let at = {
            let mut cursor = self.cursor.lock().unwrap_or_else(|e| e.into_inner());
            let Cursor {
                stt_next, stt_used, ..
            } = &mut *cursor;
            choose(&prints, self.mode, stt_next, stt_used, &got)?
        };
        let call = calls
            .iter()
            .find(|c| c.at == at)
            .ok_or(ProviderError::NotReady)?;
        let mut text = String::new();
        for event in call.events {
            if let TranscriptEvent::Final { text: t, .. } = event {
                text.push_str(&t.0);
            }
            if sink.event(event.clone()) == Flow::Stop {
                return Ok(SttEnd {
                    text: HeardText(text),
                    audio: got.audio.duration,
                    served: self.cassette.header.model.clone(),
                });
            }
        }
        call.end.clone()
    }
}

impl TextToSpeech for SpeechReplay {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(self.tts_info()))
    }

    /// Plays silence of the recorded duration, in 20 ms chunks, then the recorded end. A sink
    /// that stops it gets `Ok` with the audio played so far.
    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send {
        let got = TtsPrint::of(request);
        let calls = self.tts_calls();
        let prints: Vec<(usize, &TtsPrint)> = calls.iter().map(|c| (c.at, c.print)).collect();
        let picked = {
            let mut cursor = self.cursor.lock().unwrap_or_else(|e| e.into_inner());
            let Cursor {
                tts_next, tts_used, ..
            } = &mut *cursor;
            choose(&prints, self.mode, tts_next, tts_used, &got)
        };
        let result = picked.and_then(|at| {
            let call = calls
                .iter()
                .find(|c| c.at == at)
                .ok_or(ProviderError::NotReady)?;
            let mut played = 0u64;
            for chunk in silence(call.audio.format, call.audio.duration) {
                played += u64::from(chunk.duration().0);
                if sink.chunk(chunk) == Flow::Stop {
                    return Ok(TtsEnd {
                        audio: AudioMs(u32::try_from(played).unwrap_or(u32::MAX)),
                        served: self.cassette.header.model.clone(),
                    });
                }
            }
            call.end.clone()
        });
        std::future::ready(result)
    }
}

/// A sink for recorded speech interactions, injected so the crate itself writes no files.
pub trait SpeechCassetteSink: Send + Sync {
    fn write(&self, line: &SpeechInteraction) -> Result<(), SinkError>;
}

/// Wraps a speech provider and records every call into a sink. A sink that refuses the write
/// fails the call (`Unreadable`).
#[derive(Debug)]
pub struct RecordingSpeech<P, C: SpeechCassetteSink> {
    inner: P,
    sink: C,
}

impl<P, C: SpeechCassetteSink> RecordingSpeech<P, C> {
    pub fn new(inner: P, sink: C) -> Self {
        Self { inner, sink }
    }

    fn write(&self, line: SpeechInteraction) -> Result<(), ProviderError> {
        self.sink
            .write(&line)
            .map_err(|_| ProviderError::Unreadable("the cassette sink refused the write".into()))
    }
}

/// Passes audio through and keeps a copy for the print.
struct TeeSource<'a, A: AudioSource> {
    inner: &'a mut A,
    seen: Vec<AudioChunk>,
}

impl<A: AudioSource> AudioSource for TeeSource<'_, A> {
    async fn next(&mut self) -> AudioPull {
        let pull = self.inner.next().await;
        if let AudioPull::Chunk(chunk) = &pull {
            self.seen.push(chunk.clone());
        }
        pull
    }
}

struct TeeTranscript<'a, K: TranscriptSink> {
    inner: &'a mut K,
    seen: Vec<TranscriptEvent>,
}

impl<K: TranscriptSink> TranscriptSink for TeeTranscript<'_, K> {
    fn event(&mut self, event: TranscriptEvent) -> Flow {
        self.seen.push(event.clone());
        self.inner.event(event)
    }
}

struct TeeAudio<'a, K: AudioSink> {
    inner: &'a mut K,
    seen: Vec<AudioChunk>,
}

impl<K: AudioSink> AudioSink for TeeAudio<'_, K> {
    fn chunk(&mut self, chunk: AudioChunk) -> Flow {
        self.seen.push(chunk.clone());
        self.inner.chunk(chunk)
    }
}

impl<P: SpeechToText, C: SpeechCassetteSink> SpeechToText for RecordingSpeech<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    async fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> Result<SttEnd, ProviderError> {
        let mut source = TeeSource {
            inner: audio,
            seen: Vec::new(),
        };
        let mut events = TeeTranscript {
            inner: sink,
            seen: Vec::new(),
        };
        let end = self
            .inner
            .transcribe(request, &mut source, &mut events)
            .await;
        self.write(SpeechInteraction::Stt {
            request: SttPrint::of(request, &source.seen),
            events: events.seen,
            end: end.clone(),
        })?;
        end
    }
}

impl<P: TextToSpeech, C: SpeechCassetteSink> TextToSpeech for RecordingSpeech<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    async fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> Result<TtsEnd, ProviderError> {
        let mut tee = TeeAudio {
            inner: sink,
            seen: Vec::new(),
        };
        let end = self.inner.speak(request, &mut tee).await;
        self.write(SpeechInteraction::Tts {
            request: TtsPrint::of(request),
            audio: AudioPrint::of(&tee.seen),
            end: end.clone(),
        })?;
        end
    }
}
