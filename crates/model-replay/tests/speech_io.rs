//! Speech replay and recording: prints, silence of the recorded duration, tees.

use std::collections::VecDeque;
use std::sync::Mutex;

use model_provider::{Flow, ModelName, ProviderError, Support};
use model_replay::{
    AudioPrint, BuildLabel, CassetteHeader, CassetteVersion, EngineLabel, EngineStamp, RecordedAt,
    RecordingSpeech, ReplayMode, SinkError, SpeechCassette, SpeechCassetteSink, SpeechInteraction,
    SpeechReplay, SttPrint, TtsPrint,
};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, AudioPull, AudioSink, AudioSource, HeardText, Lang,
    LangChoice, PcmBytes, PcmFormat, SampleIndex, SampleRate, SpeechIo, SpeechToText, SpokenText,
    SttEnd, SttMode, SttRequest, TextToSpeech, TranscriptEvent, TranscriptSink, TtsEnd, TtsRequest,
    VoiceId,
};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("speech replay never waits"),
    }
}

const S16_16K: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};
const S16_24K: AudioFormat = AudioFormat {
    rate: SampleRate(24_000),
    pcm: PcmFormat::S16Le,
};

fn chunk(at: u64, bytes: Vec<u8>) -> AudioChunk {
    AudioChunk {
        format: S16_16K,
        at: SampleIndex(at),
        pcm: PcmBytes::new(bytes),
    }
}

struct Pulls(VecDeque<AudioChunk>);

impl AudioSource for Pulls {
    fn next(&mut self) -> impl Future<Output = AudioPull> + Send {
        std::future::ready(self.0.pop_front().map_or(AudioPull::End, AudioPull::Chunk))
    }
}

fn stt_request() -> SttRequest {
    SttRequest {
        model: ModelName("nemotron".into()),
        mode: SttMode::Streaming {
            chunk: AudioMs(560),
        },
        lang: LangChoice::Auto,
        format: S16_16K,
    }
}

fn tts_request(text: &str) -> TtsRequest {
    TtsRequest {
        model: ModelName("kokoro".into()),
        text: SpokenText::new(text).unwrap(),
        voice: VoiceId::new("af_heart").unwrap(),
        lang: Lang::new("en-US").unwrap(),
        format: S16_24K,
    }
}

/// 1 second of 16 kHz S16 audio in two chunks.
fn second() -> Vec<AudioChunk> {
    vec![chunk(0, vec![1; 16_000]), chunk(8_000, vec![2; 16_000])]
}

struct Events(Vec<TranscriptEvent>, usize);

impl TranscriptSink for Events {
    fn event(&mut self, e: TranscriptEvent) -> Flow {
        self.0.push(e);
        if self.0.len() >= self.1 {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

struct Chunks(Vec<AudioChunk>, usize);

impl AudioSink for Chunks {
    fn chunk(&mut self, c: AudioChunk) -> Flow {
        self.0.push(c);
        if self.0.len() >= self.1 {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

fn events() -> Vec<TranscriptEvent> {
    vec![
        TranscriptEvent::Partial {
            text: HeardText("hel".into()),
            from: SampleIndex(0),
        },
        TranscriptEvent::Final {
            text: HeardText("hello".into()),
            from: SampleIndex(0),
            to: SampleIndex(16_000),
        },
    ]
}

fn stt_end() -> SttEnd {
    SttEnd {
        text: HeardText("hello".into()),
        audio: AudioMs(1000),
        served: ModelName("nemotron".into()),
    }
}

fn header() -> CassetteHeader {
    CassetteHeader {
        vocab: CassetteVersion::CURRENT,
        engine: EngineStamp {
            kind: EngineLabel("speech_host".into()),
            build: BuildLabel("0".into()),
        },
        model: ModelName("nemotron".into()),
        recorded: RecordedAt(1),
    }
}

fn stt_line() -> SpeechInteraction {
    SpeechInteraction::Stt {
        request: SttPrint::of(&stt_request(), &second()),
        events: events(),
        end: Ok(stt_end()),
    }
}

fn tts_line(text: &str, ms: u32, end: Result<TtsEnd, ProviderError>) -> SpeechInteraction {
    SpeechInteraction::Tts {
        request: TtsPrint::of(&tts_request(text)),
        audio: AudioPrint {
            format: S16_24K,
            digest: speech_provider_digest(),
            len: model_replay::ByteCount(u64::from(ms) * 48),
            duration: AudioMs(ms),
        },
        end,
    }
}

fn speech_provider_digest() -> model_replay::AudioDigest {
    model_replay::AudioDigest("00".repeat(32))
}

fn tts_end(ms: u32) -> Result<TtsEnd, ProviderError> {
    Ok(TtsEnd {
        audio: AudioMs(ms),
        served: ModelName("kokoro".into()),
    })
}

fn replay(lines: Vec<SpeechInteraction>, mode: ReplayMode) -> SpeechReplay {
    SpeechReplay::new(
        SpeechCassette {
            header: header(),
            interactions: lines,
        },
        mode,
    )
}

#[test]
fn audio_print_is_chunking_independent_and_sums_length_and_duration() {
    let whole = AudioPrint::of(&second());
    let one = AudioPrint::of(&[chunk(0, [vec![1; 16_000], vec![2; 16_000]].concat())]);
    assert_eq!(whole, one);
    assert_eq!(whole.len.0, 32_000);
    assert_eq!(whole.duration, AudioMs(1000));
    assert_eq!(whole.digest.0.len(), 64);
    assert_ne!(
        whole.digest,
        AudioPrint::of(&[chunk(0, vec![9; 32_000])]).digest
    );
    let empty = AudioPrint::of(&[]);
    assert_eq!((empty.len.0, empty.duration), (0, AudioMs(0)));
}

#[test]
fn prints_hold_digests_not_content() {
    let tts = TtsPrint::of(&tts_request("hello there"));
    assert_eq!(tts.text.len.0, 11);
    assert_eq!(
        tts.text.digest.0,
        blake3::hash(b"hello there").to_hex().to_string()
    );
    let json = serde_json::to_string(&(tts, SttPrint::of(&stt_request(), &second()))).unwrap();
    assert!(!json.contains("hello there"));
}

#[test]
fn transcribe_drains_the_audio_and_replays_events_and_end() {
    let r = replay(vec![stt_line()], ReplayMode::Strict);
    let mut audio = Pulls(second().into());
    let mut sink = Events(vec![], usize::MAX);
    let end = block_on(r.transcribe(&stt_request(), &mut audio, &mut sink)).unwrap();
    assert!(audio.0.is_empty(), "all audio was pulled");
    assert_eq!(sink.0, events());
    assert_eq!(end, stt_end());
}

#[test]
fn transcribe_refuses_other_audio_in_strict_and_by_request() {
    for mode in [ReplayMode::Strict, ReplayMode::ByRequest] {
        let r = replay(vec![stt_line()], mode);
        let mut audio = Pulls(vec![chunk(0, vec![7; 100])].into());
        let got =
            block_on(r.transcribe(&stt_request(), &mut audio, &mut Events(vec![], usize::MAX)));
        assert!(
            matches!(got, Err(ProviderError::BadRequest(m)) if m.contains("interaction 0")),
            "{mode:?}"
        );
    }
    let r = replay(vec![stt_line()], ReplayMode::InOrder);
    let mut audio = Pulls(vec![chunk(0, vec![7; 100])].into());
    assert!(
        block_on(r.transcribe(&stt_request(), &mut audio, &mut Events(vec![], usize::MAX))).is_ok()
    );
    let mut audio = Pulls(VecDeque::new());
    assert_eq!(
        block_on(r.transcribe(&stt_request(), &mut audio, &mut Events(vec![], usize::MAX))),
        Err(ProviderError::NotReady)
    );
}

#[test]
fn a_stopping_sink_ends_transcription_with_the_text_so_far() {
    let r = replay(vec![stt_line()], ReplayMode::InOrder);
    let mut sink = Events(vec![], 2);
    let end =
        block_on(r.transcribe(&stt_request(), &mut Pulls(second().into()), &mut sink)).unwrap();
    assert_eq!(end.text, HeardText("hello".into()));
    assert_eq!(end.audio, AudioMs(1000));
}

#[test]
fn speak_plays_silence_of_the_recorded_duration() {
    let r = replay(
        vec![tts_line("hi", 1000, tts_end(1000))],
        ReplayMode::Strict,
    );
    let mut sink = Chunks(vec![], usize::MAX);
    let end = block_on(r.speak(&tts_request("hi"), &mut sink)).unwrap();
    assert_eq!(end.audio, AudioMs(1000));
    assert_eq!(sink.0.len(), 50, "20 ms chunks");
    let total: u32 = sink.0.iter().map(|c| c.duration().0).sum();
    assert_eq!(total, 1000);
    assert!(
        sink.0
            .iter()
            .all(|c| c.format == S16_24K && c.pcm.as_slice().iter().all(|b| *b == 0))
    );
    assert_eq!(sink.0[1].at, SampleIndex(480));
}

#[test]
fn speak_handles_odd_durations_barge_in_and_recorded_errors() {
    let r = replay(vec![tts_line("hi", 30, tts_end(30))], ReplayMode::InOrder);
    let mut sink = Chunks(vec![], usize::MAX);
    block_on(r.speak(&tts_request("hi"), &mut sink)).unwrap();
    assert_eq!(
        sink.0.iter().map(|c| c.duration().0).collect::<Vec<_>>(),
        [20, 10]
    );

    let r = replay(
        vec![tts_line("hi", 1000, tts_end(1000))],
        ReplayMode::InOrder,
    );
    let mut sink = Chunks(vec![], 3);
    let end = block_on(r.speak(&tts_request("hi"), &mut sink)).unwrap();
    assert_eq!((sink.0.len(), end.audio), (3, AudioMs(60)));

    let r = replay(
        vec![tts_line("hi", 40, Err(ProviderError::Timeout))],
        ReplayMode::InOrder,
    );
    let mut sink = Chunks(vec![], usize::MAX);
    assert_eq!(
        block_on(r.speak(&tts_request("hi"), &mut sink)),
        Err(ProviderError::Timeout)
    );
    assert_eq!(
        sink.0.len(),
        2,
        "the audio made before the failure still plays"
    );

    let r = replay(vec![tts_line("hi", 0, tts_end(0))], ReplayMode::InOrder);
    let mut sink = Chunks(vec![], usize::MAX);
    block_on(r.speak(&tts_request("hi"), &mut sink)).unwrap();
    assert!(sink.0.is_empty());
}

#[test]
fn directions_are_counted_apart_and_by_request_finds_any_order() {
    let lines = vec![
        tts_line("one", 20, tts_end(20)),
        stt_line(),
        tts_line("two", 20, tts_end(20)),
    ];
    let r = replay(lines.clone(), ReplayMode::Strict);
    let mut sink = Chunks(vec![], usize::MAX);
    block_on(r.speak(&tts_request("one"), &mut sink)).unwrap();
    block_on(r.transcribe(
        &stt_request(),
        &mut Pulls(second().into()),
        &mut Events(vec![], usize::MAX),
    ))
    .unwrap();
    block_on(r.speak(&tts_request("two"), &mut sink)).unwrap();

    let r = replay(lines, ReplayMode::ByRequest);
    block_on(r.speak(&tts_request("two"), &mut sink)).unwrap();
    block_on(r.speak(&tts_request("one"), &mut sink)).unwrap();
    assert_eq!(
        block_on(r.speak(&tts_request("one"), &mut sink)),
        Err(ProviderError::NotReady)
    );
}

#[test]
fn describe_reports_what_the_calls_show() {
    let lines = vec![
        stt_line(),
        tts_line("a", 500, tts_end(500)),
        tts_line("b", 900, tts_end(900)),
    ];
    let r = replay(lines, ReplayMode::InOrder);
    let stt = block_on(SpeechToText::describe(&r)).unwrap();
    assert_eq!(stt.len(), 1);
    assert_eq!(stt[0].name, ModelName("nemotron".into()));
    assert_eq!(stt[0].caps.streaming, Support::Present);
    assert_eq!(stt[0].caps.partials, Support::Present);
    assert_eq!(stt[0].caps.io, SpeechIo::In { input: S16_16K });
    let tts = block_on(TextToSpeech::describe(&r)).unwrap();
    assert_eq!(tts[0].caps.max_audio, AudioMs(900));
    assert_eq!(
        tts[0].caps.io,
        SpeechIo::Out {
            output: S16_24K,
            voices: vec![VoiceId::new("af_heart").unwrap()]
        }
    );
    let none = replay(vec![], ReplayMode::InOrder);
    assert!(block_on(SpeechToText::describe(&none)).unwrap().is_empty());
    assert!(block_on(TextToSpeech::describe(&none)).unwrap().is_empty());
}

#[derive(Default)]
struct Lines(Mutex<Vec<SpeechInteraction>>);

impl SpeechCassetteSink for &Lines {
    fn write(&self, line: &SpeechInteraction) -> Result<(), SinkError> {
        self.0.lock().unwrap().push(line.clone());
        Ok(())
    }
}

struct Refuse;

impl SpeechCassetteSink for Refuse {
    fn write(&self, _: &SpeechInteraction) -> Result<(), SinkError> {
        Err(SinkError)
    }
}

#[test]
fn recording_a_replay_gives_back_the_cassette() {
    let lines = Lines::default();
    let rec = RecordingSpeech::new(
        replay(
            vec![stt_line(), tts_line("hi", 100, tts_end(100))],
            ReplayMode::InOrder,
        ),
        &lines,
    );
    let mut events = Events(vec![], usize::MAX);
    block_on(rec.transcribe(&stt_request(), &mut Pulls(second().into()), &mut events)).unwrap();
    let mut audio = Chunks(vec![], usize::MAX);
    block_on(rec.speak(&tts_request("hi"), &mut audio)).unwrap();
    assert_eq!(events.0, self::events(), "the caller still sees the events");
    assert_eq!(audio.0.len(), 5);

    let got = lines.0.lock().unwrap().clone();
    assert_eq!(got[0], stt_line());
    let SpeechInteraction::Tts {
        request,
        audio: print,
        end,
    } = &got[1]
    else {
        panic!("a synthesis")
    };
    assert_eq!(*request, TtsPrint::of(&tts_request("hi")));
    assert_eq!((print.duration, print.len.0), (AudioMs(100), 4800));
    assert_eq!(*end, tts_end(100));
}

#[test]
fn a_refusing_sink_fails_the_call() {
    let rec = RecordingSpeech::new(
        replay(vec![tts_line("hi", 20, tts_end(20))], ReplayMode::InOrder),
        Refuse,
    );
    let got = block_on(rec.speak(&tts_request("hi"), &mut Chunks(vec![], usize::MAX)));
    assert!(matches!(got, Err(ProviderError::Unreadable(_))));
}
