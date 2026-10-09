use std::collections::BTreeSet;

use model_provider::{ModelName, ProviderError, RetrySeconds, Support};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, FRAME_SAMPLES, Frame512, FrameError, FrameLenError,
    HeardText, HostIn, HostOut, HostVocab, Lang, LangChoice, LangSet, MAX_FRAME_BYTES, PcmBytes,
    PcmFormat, SampleIndex, SampleRate, SpeechCaps, SpeechDir, SpeechIo, SpeechModelInfo,
    SpokenText, SttEnd, SttMode, SttRequest, TranscriptEvent, TtsRequest, VoiceId, decode_frame,
    encode_frame, frame_length,
};

const S16_16K: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

fn lang(tag: &str) -> Lang {
    Lang::new(tag).unwrap()
}

fn chunk(bytes: Vec<u8>) -> AudioChunk {
    AudioChunk {
        format: S16_16K,
        at: SampleIndex(320),
        pcm: PcmBytes::new(bytes),
    }
}

fn stt_request() -> SttRequest {
    SttRequest {
        model: ModelName("nemotron-3.5-asr-streaming".into()),
        mode: SttMode::Streaming {
            chunk: AudioMs(560),
        },
        lang: LangChoice::Prefer(vec![lang("zh-CN"), lang("en-US")]),
        format: S16_16K,
    }
}

fn in_caps() -> SpeechCaps {
    SpeechCaps {
        streaming: Support::Present,
        partials: Support::Present,
        punctuation: Support::Present,
        timestamps: Support::Absent,
        langs: LangSet::Listed(BTreeSet::from([lang("en-US"), lang("zh-CN")])),
        max_audio: AudioMs(120_000),
        io: SpeechIo::In { input: S16_16K },
    }
}

fn out_caps() -> SpeechCaps {
    SpeechCaps {
        streaming: Support::Present,
        partials: Support::Absent,
        punctuation: Support::Absent,
        timestamps: Support::Absent,
        langs: LangSet::Any,
        max_audio: AudioMs(60_000),
        io: SpeechIo::Out {
            output: AudioFormat {
                rate: SampleRate(24_000),
                pcm: PcmFormat::S16Le,
            },
            voices: vec![VoiceId::new("af_heart").unwrap()],
        },
    }
}

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + core::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), value, "{json}");
}

#[test]
fn speech_types_round_trip() {
    let events = vec![
        TranscriptEvent::Partial {
            text: HeardText("hel".into()),
            from: SampleIndex(0),
        },
        TranscriptEvent::Final {
            text: HeardText("hello".into()),
            from: SampleIndex(0),
            to: SampleIndex(16_000),
        },
        TranscriptEvent::Lang(lang("en")),
    ];
    let end = SttEnd {
        text: HeardText("hello".into()),
        audio: AudioMs(1000),
        served: ModelName("nemotron-3.5-asr-streaming".into()),
    };
    let info = SpeechModelInfo {
        name: ModelName("m".into()),
        caps: in_caps(),
    };
    for event in &events {
        round_trip(event);
    }
    let host_in = vec![
        HostIn::Hello {
            vocab: HostVocab::CURRENT,
        },
        HostIn::Begin(stt_request()),
        HostIn::Audio(chunk(vec![0, 1, 2, 3])),
        HostIn::End,
        HostIn::Cancel,
    ];
    for message in &host_in {
        round_trip(message);
    }
    let host_out = vec![
        HostOut::Hello {
            vocab: HostVocab::CURRENT,
            models: vec![info],
        },
        HostOut::Event(events[0].clone()),
        HostOut::Done(end),
        HostOut::Failed(ProviderError::RateLimited(RetrySeconds(3))),
    ];
    for message in &host_out {
        round_trip(message);
    }
    round_trip(&in_caps());
    round_trip(&out_caps());
    round_trip(&stt_request());
    round_trip(&LangChoice::Auto);
    round_trip(&SttMode::Batch);
}

#[test]
fn wire_json_is_pinned() {
    assert_eq!(
        serde_json::to_string(&HostIn::End).unwrap(),
        r#"{"kind":"end"}"#
    );
    assert_eq!(
        serde_json::to_string(&HostIn::Hello {
            vocab: HostVocab::CURRENT
        })
        .unwrap(),
        r#"{"kind":"hello","v":{"vocab":1}}"#
    );
    assert_eq!(
        serde_json::to_string(&HostIn::Audio(chunk(vec![0xff, 0xd8, 0xff]))).unwrap(),
        r#"{"kind":"audio","v":{"format":{"rate":16000,"pcm":"s16_le"},"at":320,"pcm":"/9j/"}}"#
    );
    assert_eq!(
        serde_json::to_string(&TranscriptEvent::Final {
            text: HeardText("hi".into()),
            from: SampleIndex(1),
            to: SampleIndex(2),
        })
        .unwrap(),
        r#"{"kind":"final","v":{"text":"hi","from":1,"to":2}}"#
    );
    assert_eq!(
        serde_json::to_string(&stt_request()).unwrap(),
        r#"{"model":"nemotron-3.5-asr-streaming","mode":{"kind":"streaming","v":{"chunk":560}},"lang":{"kind":"prefer","v":["zh-CN","en-US"]},"format":{"rate":16000,"pcm":"s16_le"}}"#
    );
    assert_eq!(
        serde_json::to_string(&out_caps()).unwrap(),
        r#"{"streaming":"present","partials":"absent","punctuation":"absent","timestamps":"absent","langs":{"kind":"any"},"max_audio_ms":60000,"dir":"out","output":{"rate":24000,"pcm":"s16_le"},"voices":["af_heart"]}"#
    );
}

#[test]
fn caps_know_their_direction() {
    assert_eq!(in_caps().dir(), SpeechDir::In);
    assert_eq!(out_caps().dir(), SpeechDir::Out);
}

#[test]
fn pcm_is_base64_and_never_prints() {
    let pcm = PcmBytes::new(vec![0xff, 0xd8, 0xff]);
    assert_eq!(serde_json::to_string(&pcm).unwrap(), r#""/9j/""#);
    assert!(serde_json::from_str::<PcmBytes>(r#""!!""#).is_err());
    assert_eq!(format!("{pcm:?}"), "PcmBytes(<3 bytes>)");
}

#[test]
fn pcm_debug_redacts() {
    let c = chunk(vec![0xAB; 64]);
    let printed = format!("{c:?}");
    assert!(printed.contains("<64 bytes>"), "{printed}");
    assert!(
        !printed.contains("171") && !printed.to_lowercase().contains("ab,"),
        "{printed}"
    );
    assert_eq!(
        format!("{:?}", HeardText("secret words".into())),
        "HeardText(<12 bytes>)"
    );
    assert_eq!(
        format!("{:?}", SpokenText::new("secret words").unwrap()),
        "SpokenText(<12 bytes>)"
    );
    let req = TtsRequest {
        model: ModelName("kokoro-82m".into()),
        text: SpokenText::new("private").unwrap(),
        voice: VoiceId::new("af_heart").unwrap(),
        lang: lang("en-US"),
        format: S16_16K,
    };
    assert!(!format!("{req:?}").contains("private"));
    let frame = Frame512::new(&[7; FRAME_SAMPLES]).unwrap();
    assert_eq!(format!("{frame:?}"), "Frame512(<512 samples>)");
}

#[test]
fn chunk_duration_table() {
    // (rate, pcm, bytes, samples, ms)
    const CASES: &[(u32, PcmFormat, usize, u32, u32)] = &[
        (16_000, PcmFormat::S16Le, 0, 0, 0),
        (16_000, PcmFormat::S16Le, 1024, 512, 32),
        (16_000, PcmFormat::S16Le, 1025, 512, 32),
        (16_000, PcmFormat::S16Le, 32_000, 16_000, 1000),
        (16_000, PcmFormat::F32Le, 2048, 512, 32),
        (24_000, PcmFormat::S16Le, 48_000, 24_000, 1000),
        (24_000, PcmFormat::S16Le, 2, 1, 0),
        (44_100, PcmFormat::S16Le, 88_200, 44_100, 1000),
        (0, PcmFormat::S16Le, 100, 50, 0),
    ];
    for &(rate, pcm, bytes, samples, ms) in CASES {
        let c = AudioChunk {
            format: AudioFormat {
                rate: SampleRate(rate),
                pcm,
            },
            at: SampleIndex(0),
            pcm: PcmBytes::new(vec![0; bytes]),
        };
        assert_eq!(
            (c.samples(), c.duration()),
            (samples, AudioMs(ms)),
            "{rate} {pcm:?} {bytes}"
        );
    }
}

#[test]
fn languages_and_voices_are_checked() {
    for good in ["en", "zh-TW", "zh-Hant-TW", "en-US"] {
        assert!(Lang::new(good).is_ok(), "{good}");
    }
    for bad in ["", "en_US", "en US", "日本語", &"a".repeat(36)] {
        assert!(Lang::new(bad).is_err(), "{bad}");
    }
    assert!(serde_json::from_str::<Lang>(r#""en_US""#).is_err());
    assert!(VoiceId::new("af_heart").is_ok());
    assert!(VoiceId::new("").is_err());
    assert!(VoiceId::new("a b").is_err());
    assert!(SpokenText::new("x".repeat(4096)).is_ok());
    assert!(SpokenText::new("x".repeat(4097)).is_err());
    assert!(SpokenText::new("").is_err());
    assert!(serde_json::from_str::<SpokenText>(r#""""#).is_err());
}

#[test]
fn a_frame_is_exactly_512_samples() {
    assert!(Frame512::new(&[0; 512]).is_ok());
    assert_eq!(Frame512::new(&[0; 511]).unwrap_err(), FrameLenError(511));
    assert_eq!(Frame512::new(&[0; 513]).unwrap_err(), FrameLenError(513));
}

#[test]
fn frames_are_length_prefixed_and_capped() {
    let framed = encode_frame(&HostIn::End).unwrap();
    assert_eq!(&framed[..4], &[0, 0, 0, 14]);
    assert_eq!(&framed[4..], br#"{"kind":"end"}"#);
    let len = frame_length([framed[0], framed[1], framed[2], framed[3]]).unwrap();
    assert_eq!(
        decode_frame::<HostIn>(&framed[4..4 + len]).unwrap(),
        HostIn::End
    );

    let over = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    assert_eq!(
        frame_length(over),
        Err(FrameError::TooLarge {
            len: MAX_FRAME_BYTES + 1
        })
    );
    assert!(frame_length((MAX_FRAME_BYTES as u32).to_be_bytes()).is_ok());
    assert!(matches!(
        decode_frame::<HostIn>(b"{nope"),
        Err(FrameError::Json(_))
    ));
    let big = chunk(vec![0; MAX_FRAME_BYTES]);
    assert!(matches!(
        encode_frame(&HostIn::Audio(big)),
        Err(FrameError::TooLarge { .. })
    ));
}
