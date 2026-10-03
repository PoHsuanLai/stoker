//! Fakes that play scripts and record every request, for tests of the daemons above.

use std::sync::Mutex;

use model_provider::{Flow, ProviderError};

use crate::{
    AudioChunk, AudioMs, AudioPull, AudioSink, AudioSource, Frame512, PcmBytes, SampleIndex,
    SpeechModelInfo, SpeechProb, SpeechToText, SttEnd, SttRequest, TextToSpeech, TranscriptEvent,
    TranscriptSink, TtsEnd, TtsRequest, VoiceActivity, Voiced,
};

/// Plays `(index, event)` pairs: each event is pushed once the audio pulled so far passes its
/// index. Records every request.
#[derive(Debug)]
pub struct ScriptedStt {
    models: Vec<SpeechModelInfo>,
    script: Vec<(SampleIndex, TranscriptEvent)>,
    end: Result<SttEnd, ProviderError>,
    seen: Mutex<Vec<SttRequest>>,
}

impl ScriptedStt {
    pub fn new(
        models: Vec<SpeechModelInfo>,
        script: Vec<(SampleIndex, TranscriptEvent)>,
        end: Result<SttEnd, ProviderError>,
    ) -> Self {
        Self {
            models,
            script,
            end,
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every request the fake has been given, in order.
    pub fn requests(&self) -> Vec<SttRequest> {
        self.seen.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl SpeechToText for ScriptedStt {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(self.models.clone()))
    }

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send {
        // The request is kept before the script plays.
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(request.clone());
        }
        async move {
            let mut next = 0;
            let mut heard = 0_u64;
            loop {
                let pulled = audio.next().await;
                let (position, last) = match pulled {
                    AudioPull::Chunk(chunk) => (
                        Some(chunk.at.0.saturating_add(u64::from(chunk.samples()))),
                        false,
                    ),
                    AudioPull::End => (None, true),
                };
                heard = position.map_or(heard, |p| heard.max(p));
                // Each event goes out once, when the audio pulled so far reaches its index; at
                // the end of the audio everything left goes out.
                while let Some((index, event)) = self.script.get(next) {
                    if !last && index.0 > heard {
                        break;
                    }
                    next += 1;
                    if sink.event(event.clone()) == Flow::Stop {
                        return self.end.clone();
                    }
                }
                if last {
                    return self.end.clone();
                }
            }
        }
    }
}

/// What one scripted synthesis makes: silence of a length, in chunks of a length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtsScript {
    pub audio: AudioMs,
    pub chunk: AudioMs,
}

/// Emits silence per request and records the requests.
#[derive(Debug)]
pub struct ScriptedTts {
    models: Vec<SpeechModelInfo>,
    script: TtsScript,
    seen: Mutex<Vec<TtsRequest>>,
}

impl ScriptedTts {
    pub fn new(models: Vec<SpeechModelInfo>, script: TtsScript) -> Self {
        Self {
            models,
            script,
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<TtsRequest> {
        self.seen.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl TextToSpeech for ScriptedTts {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(self.models.clone()))
    }

    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(request.clone());
        }
        let rate = u64::from(request.format.rate.0);
        let width = u64::from(request.format.pcm.width());
        let samples_in = |ms: AudioMs| u64::from(ms.0) * rate / 1000;
        let total = samples_in(self.script.audio);
        // A chunk length of zero is the whole audio in one chunk.
        let step = Some(samples_in(self.script.chunk))
            .filter(|n| *n > 0)
            .unwrap_or(total.max(1));
        let (format, served, audio) = (request.format, request.model.clone(), self.script.audio);
        async move {
            let mut at = 0_u64;
            while at < total {
                let count = step.min(total - at);
                let bytes = usize::try_from(count * width).unwrap_or(usize::MAX);
                let chunk = AudioChunk {
                    format,
                    at: SampleIndex(at),
                    pcm: PcmBytes::new(vec![0; bytes]),
                };
                at += count;
                if sink.chunk(chunk) == Flow::Stop {
                    let ms = if rate == 0 { 0 } else { at * 1000 / rate };
                    return Ok(TtsEnd {
                        audio: AudioMs(u32::try_from(ms).unwrap_or(u32::MAX)),
                        served,
                    });
                }
            }
            Ok(TtsEnd { audio, served })
        }
    }
}

/// Answers one scripted verdict per frame; silence once the script runs out.
#[derive(Debug)]
pub struct ScriptedVad {
    verdicts: Vec<Voiced>,
    next: usize,
}

impl ScriptedVad {
    pub fn new(verdicts: Vec<Voiced>) -> Self {
        Self { verdicts, next: 0 }
    }
}

impl VoiceActivity for ScriptedVad {
    fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        let _ = frame;
        let verdict = self
            .verdicts
            .get(self.next)
            .copied()
            .unwrap_or(Voiced::Silence);
        self.next = self.next.saturating_add(1);
        let probability = match verdict {
            Voiced::Speech => 1000,
            Voiced::Silence => 0,
        };
        (verdict, SpeechProb(probability))
    }

    fn reset(&mut self) {
        self.next = 0;
    }
}
