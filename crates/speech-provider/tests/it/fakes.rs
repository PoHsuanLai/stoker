//! The scripted fakes: events by audio offset, silence by length, one verdict per frame.
#![cfg(feature = "testing")]

use std::collections::VecDeque;
use std::sync::Mutex;

use model_provider::{Flow, ModelName, ProviderError};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, AudioPull, AudioSink, AudioSource, Frame512, HeardText, Lang,
    LangChoice, PcmBytes, PcmFormat, SampleIndex, SampleRate, ScriptedStt, ScriptedTts,
    ScriptedVad, SpeechToText, SpokenText, SttEnd, SttMode, SttRequest, TextToSpeech,
    TranscriptEvent, TranscriptSink, TtsEnd, TtsRequest, TtsScript, VoiceActivity, VoiceId, Voiced,
};

const S16: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("these fakes never wait"),
    }
}

fn heard(text: &str) -> HeardText {
    HeardText(text.into())
}

fn final_event(text: &str, to: u64) -> TranscriptEvent {
    TranscriptEvent::Final {
        text: heard(text),
        from: SampleIndex(0),
        to: SampleIndex(to),
    }
}

fn request() -> SttRequest {
    SttRequest {
        model: ModelName("asr".into()),
        mode: SttMode::Streaming {
            chunk: AudioMs(560),
        },
        lang: LangChoice::Auto,
        format: S16,
    }
}

fn end() -> SttEnd {
    SttEnd {
        text: heard("one two three"),
        audio: AudioMs(300),
        served: ModelName("asr".into()),
    }
}

/// Chunks of `samples` samples from sample 0, then the end; everything pulled is logged.
struct Source<'a> {
    chunks: VecDeque<AudioChunk>,
    log: &'a Mutex<Vec<String>>,
    on_first_pull: Option<Box<dyn Fn() + Send + 'a>>,
}

fn chunks(samples: usize, count: usize) -> VecDeque<AudioChunk> {
    (0..count)
        .map(|i| AudioChunk {
            format: S16,
            at: SampleIndex((i * samples) as u64),
            pcm: PcmBytes::new(vec![0; samples * 2]),
        })
        .collect()
}

impl AudioSource for Source<'_> {
    fn next(&mut self) -> impl Future<Output = AudioPull> + Send {
        if let Some(check) = self.on_first_pull.take() {
            check();
        }
        let pulled = match self.chunks.pop_front() {
            Some(chunk) => {
                self.log
                    .lock()
                    .unwrap()
                    .push(format!("pull {}", chunk.at.0));
                AudioPull::Chunk(chunk)
            }
            None => {
                self.log.lock().unwrap().push("end".into());
                AudioPull::End
            }
        };
        std::future::ready(pulled)
    }
}

struct Sink<'a> {
    log: &'a Mutex<Vec<String>>,
    stop_after: Option<usize>,
    seen: usize,
}

impl TranscriptSink for Sink<'_> {
    fn event(&mut self, event: TranscriptEvent) -> Flow {
        let TranscriptEvent::Final { to, .. } = event else {
            panic!("finals only")
        };
        self.log.lock().unwrap().push(format!("event {}", to.0));
        self.seen += 1;
        if self.stop_after == Some(self.seen) {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

fn stt(end: Result<SttEnd, ProviderError>) -> ScriptedStt {
    ScriptedStt::new(
        vec![],
        vec![
            (SampleIndex(1000), final_event("one", 1000)),
            (SampleIndex(2000), final_event("two", 2000)),
            (SampleIndex(9000), final_event("three", 9000)),
        ],
        end,
    )
}

fn run(
    fake: &ScriptedStt,
    stop_after: Option<usize>,
) -> (Result<SttEnd, ProviderError>, Vec<String>) {
    let log = Mutex::new(Vec::new());
    let mut source = Source {
        chunks: chunks(1600, 3),
        log: &log,
        on_first_pull: Some(Box::new(|| {
            // The request is on record before the script plays.
            assert_eq!(fake.requests().len(), 1);
        })),
    };
    let mut sink = Sink {
        log: &log,
        stop_after,
        seen: 0,
    };
    let result = block_on(fake.transcribe(&request(), &mut source, &mut sink));
    let log = log.lock().unwrap().clone();
    (result, log)
}

#[test]
fn scripted_stt_emits_by_offset() {
    let fake = stt(Ok(end()));
    let (result, log) = run(&fake, None);
    assert_eq!(result, Ok(end()));
    // 1600 samples pulled reach event 1000; 3200 reach 2000; 4800 reach nothing; the end of the
    // audio flushes the rest.
    assert_eq!(
        log,
        [
            "pull 0",
            "event 1000",
            "pull 1600",
            "event 2000",
            "pull 3200",
            "end",
            "event 9000"
        ]
    );
    assert_eq!(fake.requests(), vec![request()]);
}

#[test]
fn a_sink_that_stops_ends_the_transcription_there() {
    let fake = stt(Ok(end()));
    let (result, log) = run(&fake, Some(1));
    assert_eq!(result, Ok(end()));
    assert_eq!(log, ["pull 0", "event 1000"]);
}

#[test]
fn a_scripted_failure_comes_after_the_events() {
    let fake = stt(Err(ProviderError::Timeout));
    let (result, log) = run(&fake, None);
    assert_eq!(result, Err(ProviderError::Timeout));
    assert_eq!(log.last().map(String::as_str), Some("event 9000"));
}

#[derive(Default)]
struct Chunks {
    got: Vec<(u64, usize)>,
    stop_after: Option<usize>,
}

impl AudioSink for Chunks {
    fn chunk(&mut self, chunk: AudioChunk) -> Flow {
        self.got.push((chunk.at.0, chunk.pcm.len()));
        assert!(chunk.pcm.as_slice().iter().all(|b| *b == 0), "silence");
        if self.stop_after == Some(self.got.len()) {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

fn tts_request(format: AudioFormat) -> TtsRequest {
    TtsRequest {
        model: ModelName("kokoro".into()),
        text: SpokenText::new("hello").unwrap(),
        voice: VoiceId::new("af_heart").unwrap(),
        lang: Lang::new("en-US").unwrap(),
        format,
    }
}

fn speak(
    script: TtsScript,
    format: AudioFormat,
    stop_after: Option<usize>,
) -> (TtsEnd, Vec<(u64, usize)>, usize) {
    let fake = ScriptedTts::new(vec![], script);
    let mut sink = Chunks {
        stop_after,
        ..Chunks::default()
    };
    let end = block_on(fake.speak(&tts_request(format), &mut sink)).unwrap();
    (end, sink.got, fake.requests().len())
}

#[test]
fn scripted_tts_makes_silence_in_chunks_until_the_length() {
    let script = TtsScript {
        audio: AudioMs(100),
        chunk: AudioMs(30),
    };
    let (end, got, recorded) = speak(script, S16, None);
    // 100 ms is 1600 samples: three chunks of 480 and one of 160, two bytes a sample.
    assert_eq!(got, [(0, 960), (480, 960), (960, 960), (1440, 320)]);
    assert_eq!(
        (end.audio, end.served),
        (AudioMs(100), ModelName("kokoro".into()))
    );
    assert_eq!(recorded, 1);
    // A float format is four bytes a sample; a chunk of zero is the whole audio at once.
    let f32_format = AudioFormat {
        rate: SampleRate(24_000),
        pcm: PcmFormat::F32Le,
    };
    let (_, got, _) = speak(
        TtsScript {
            audio: AudioMs(10),
            chunk: AudioMs(0),
        },
        f32_format,
        None,
    );
    assert_eq!(got, [(0, 240 * 4)]);
    // No audio is no chunk.
    let (end, got, _) = speak(
        TtsScript {
            audio: AudioMs(0),
            chunk: AudioMs(30),
        },
        S16,
        None,
    );
    assert!((got.is_empty(), end.audio) == (true, AudioMs(0)));
}

#[test]
fn scripted_tts_stop_mid_stream() {
    let script = TtsScript {
        audio: AudioMs(100),
        chunk: AudioMs(30),
    };
    let (end, got, _) = speak(script, S16, Some(2));
    assert_eq!(got.len(), 2);
    assert_eq!(
        end.audio,
        AudioMs(60),
        "what was played before the barge-in"
    );
}

fn frame() -> Frame512 {
    Frame512::new(&[0; 512]).unwrap()
}

#[test]
fn scripted_vad_plays_its_verdicts_then_silence_and_resets() {
    let mut vad = ScriptedVad::new(vec![Voiced::Silence, Voiced::Speech, Voiced::Speech]);
    let run = |vad: &mut ScriptedVad| -> Vec<(Voiced, u16)> {
        (0..5)
            .map(|_| {
                let (v, p) = vad.push(&frame());
                (v, p.0)
            })
            .collect()
    };
    let first = run(&mut vad);
    assert_eq!(
        first,
        [
            (Voiced::Silence, 0),
            (Voiced::Speech, 1000),
            (Voiced::Speech, 1000),
            (Voiced::Silence, 0),
            (Voiced::Silence, 0)
        ]
    );
    vad.reset();
    assert_eq!(run(&mut vad), first);
}
