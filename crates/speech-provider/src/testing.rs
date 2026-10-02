//! Fakes that play scripts and record every request, for tests of the daemons above.

use std::collections::VecDeque;
use std::sync::Mutex;

use model_provider::ProviderError;

use crate::{
    AudioMs, AudioSink, AudioSource, Frame512, SampleIndex, SpeechModelInfo, SpeechProb,
    SpeechToText, SttEnd, SttRequest, TextToSpeech, TranscriptEvent, TranscriptSink, TtsEnd,
    TtsRequest, VoiceActivity, Voiced,
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
        let _ = (
            &self.script,
            &self.end,
            &self.seen,
            request,
            &mut *audio,
            &mut *sink,
        );
        async {
            todo!("ScriptedStt::transcribe: pull audio, push each event once its index passes")
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
        let _ = (&self.script, &self.seen, request, &mut *sink);
        async {
            todo!("ScriptedTts::speak: silent chunks until the audio length, stop on Flow::Stop")
        }
    }
}

/// Answers one scripted verdict per frame; silence once the script runs out.
#[derive(Debug)]
pub struct ScriptedVad {
    verdicts: VecDeque<Voiced>,
}

impl ScriptedVad {
    pub fn new(verdicts: Vec<Voiced>) -> Self {
        Self {
            verdicts: verdicts.into(),
        }
    }
}

impl VoiceActivity for ScriptedVad {
    fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        let _ = (&mut self.verdicts, frame);
        todo!("ScriptedVad::push: the next verdict, with probability 1000 or 0")
    }

    fn reset(&mut self) {
        todo!("ScriptedVad::reset: back to the start of the script")
    }
}
