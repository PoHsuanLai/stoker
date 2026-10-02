use model_provider::{
    ImageDetail, Limits, Milli, ModelName, OutputShape, ProviderError, Reasoning, Role, StopReason,
    Tokens, ToolChoice, TurnEnd, TurnEvent, TurnUsage,
};
use model_replay::{
    BackendLabel, ByteCount, Cassette, CassetteError, CassetteHeader, CassetteVersion, ImageDigest,
    ImagePrint, Interaction, MessagePrint, PartPrint, RecordedAt, RequestPrint,
};
use vision_prep::MediaType;

fn print() -> RequestPrint {
    RequestPrint {
        model: ModelName("holo".into()),
        messages: vec![MessagePrint {
            role: Role::User,
            parts: vec![
                PartPrint::Text("go".into()),
                PartPrint::Image(ImagePrint {
                    media: MediaType::Png,
                    digest: ImageDigest("ab".repeat(32)),
                    len: ByteCount(4096),
                    detail: ImageDetail::Auto,
                }),
            ],
        }],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(256),
            temperature: Milli(0),
            stop: vec![],
        },
        reasoning: Reasoning::Off,
    }
}

fn cassette() -> Cassette {
    let end = TurnEnd {
        stop: StopReason::EndTurn,
        usage: TurnUsage::default(),
        served: ModelName("holo".into()),
    };
    Cassette {
        header: CassetteHeader {
            vocab: CassetteVersion::CURRENT,
            backend: BackendLabel("vllm".into()),
            model: ModelName("holo".into()),
            recorded: RecordedAt(1_790_000_000),
        },
        interactions: vec![
            Interaction {
                request: print(),
                events: vec![TurnEvent::TextDelta("hi".into())],
                end: Ok(end),
            },
            Interaction {
                request: print(),
                events: vec![],
                end: Err(ProviderError::Timeout),
            },
        ],
    }
}

#[test]
fn cassette_round_trips_as_jsonl() {
    let text = cassette().to_jsonl();
    assert_eq!(
        text.lines().count(),
        3,
        "a header line and one line per interaction"
    );
    assert_eq!(Cassette::from_jsonl(&text).unwrap(), cassette());
}

#[test]
fn header_line_is_pinned() {
    let text = cassette().to_jsonl();
    assert_eq!(
        text.lines().next().unwrap(),
        r#"{"vocab":1,"backend":"vllm","model":"holo","recorded":1790000000}"#
    );
}

#[test]
fn images_are_digests_in_the_file() {
    let text = cassette().to_jsonl();
    assert!(text.contains(&"ab".repeat(32)));
    assert!(
        !text.contains("bytes"),
        "no image bytes field in a cassette"
    );
}

#[test]
fn bad_files_name_the_line() {
    assert_eq!(Cassette::from_jsonl(""), Err(CassetteError::Empty));
    let mut text = cassette().to_jsonl();
    text.push_str("{not json}\n");
    assert_eq!(
        Cassette::from_jsonl(&text),
        Err(CassetteError::BadLine { line: 4 })
    );
    let old = text.replacen(r#""vocab":1"#, r#""vocab":9"#, 1);
    assert_eq!(
        Cassette::from_jsonl(&old),
        Err(CassetteError::Version {
            found: CassetteVersion(9),
            want: CassetteVersion::CURRENT
        })
    );
}

mod speech {
    use model_provider::{ModelName, ProviderError};
    use model_replay::{
        AudioDigest, AudioPrint, BackendLabel, ByteCount, CassetteError, CassetteHeader,
        CassetteVersion, RecordedAt, SpeechCassette, SpeechInteraction, SttPrint, TextDigest,
        TextPrint, TtsPrint,
    };
    use speech_provider::{
        AudioFormat, AudioMs, HeardText, Lang, LangChoice, PcmFormat, SampleIndex, SampleRate,
        SttEnd, SttMode, TranscriptEvent, TtsEnd, VoiceId,
    };

    const S16_16K: AudioFormat = AudioFormat {
        rate: SampleRate(16_000),
        pcm: PcmFormat::S16Le,
    };

    fn audio(digest: &str, bytes: u64, ms: u32, format: AudioFormat) -> AudioPrint {
        AudioPrint {
            format,
            digest: AudioDigest(digest.repeat(32)),
            len: ByteCount(bytes),
            duration: AudioMs(ms),
        }
    }

    fn cassette() -> SpeechCassette {
        SpeechCassette {
            header: CassetteHeader {
                vocab: CassetteVersion::CURRENT,
                backend: BackendLabel("speech_host".into()),
                model: ModelName("nemotron-3.5-asr-streaming".into()),
                recorded: RecordedAt(1_790_000_000),
            },
            interactions: vec![
                SpeechInteraction::Stt {
                    request: SttPrint {
                        model: ModelName("nemotron-3.5-asr-streaming".into()),
                        mode: SttMode::Streaming {
                            chunk: AudioMs(560),
                        },
                        lang: LangChoice::Auto,
                        format: S16_16K,
                        audio: audio("ab", 32_000, 1000, S16_16K),
                    },
                    events: vec![
                        TranscriptEvent::Partial {
                            text: HeardText("hel".into()),
                            from: SampleIndex(0),
                        },
                        TranscriptEvent::Final {
                            text: HeardText("hello".into()),
                            from: SampleIndex(0),
                            to: SampleIndex(16_000),
                        },
                    ],
                    end: Ok(SttEnd {
                        text: HeardText("hello".into()),
                        audio: AudioMs(1000),
                        served: ModelName("nemotron-3.5-asr-streaming".into()),
                    }),
                },
                SpeechInteraction::Tts {
                    request: TtsPrint {
                        model: ModelName("kokoro-82m".into()),
                        text: TextPrint {
                            digest: TextDigest("cd".repeat(32)),
                            len: ByteCount(11),
                        },
                        voice: VoiceId::new("af_heart").unwrap(),
                        lang: Lang::new("en-US").unwrap(),
                        format: AudioFormat {
                            rate: SampleRate(24_000),
                            pcm: PcmFormat::S16Le,
                        },
                    },
                    audio: audio(
                        "ef",
                        48_000,
                        1000,
                        AudioFormat {
                            rate: SampleRate(24_000),
                            pcm: PcmFormat::S16Le,
                        },
                    ),
                    end: Err(ProviderError::Timeout),
                },
                SpeechInteraction::Tts {
                    request: TtsPrint {
                        model: ModelName("kokoro-82m".into()),
                        text: TextPrint {
                            digest: TextDigest("cd".repeat(32)),
                            len: ByteCount(11),
                        },
                        voice: VoiceId::new("af_heart").unwrap(),
                        lang: Lang::new("en-US").unwrap(),
                        format: S16_16K,
                    },
                    audio: audio("ef", 0, 0, S16_16K),
                    end: Ok(TtsEnd {
                        audio: AudioMs(0),
                        served: ModelName("kokoro-82m".into()),
                    }),
                },
            ],
        }
    }

    #[test]
    fn speech_cassette_round_trip() {
        let text = cassette().to_jsonl();
        assert_eq!(
            text.lines().count(),
            4,
            "a header and one line per interaction"
        );
        assert_eq!(SpeechCassette::from_jsonl(&text).unwrap(), cassette());
        assert_eq!(
            text.lines().next().unwrap(),
            r#"{"vocab":1,"backend":"speech_host","model":"nemotron-3.5-asr-streaming","recorded":1790000000}"#
        );
    }

    #[test]
    fn interactions_keep_audio_digests_only() {
        let text = cassette().to_jsonl();
        assert!(text.contains(&"ab".repeat(32)));
        assert!(text.contains(&"ef".repeat(32)));
        assert!(
            !text.contains("bytes"),
            "no samples field in a speech cassette"
        );
        assert!(
            !text.contains("hello world"),
            "the text to speak is a digest"
        );
        let line = text.lines().nth(2).unwrap();
        assert!(
            line.starts_with(
                r#"{"kind":"tts","v":{"request":{"model":"kokoro-82m","text":{"digest":"#
            ),
            "{line}"
        );
    }

    #[test]
    fn bad_speech_files_name_the_line() {
        assert_eq!(SpeechCassette::from_jsonl(""), Err(CassetteError::Empty));
        let mut text = cassette().to_jsonl();
        text.push_str("{not json}\n");
        assert_eq!(
            SpeechCassette::from_jsonl(&text),
            Err(CassetteError::BadLine { line: 5 })
        );
        // A chat interaction is not a speech one.
        let header = cassette().to_jsonl().lines().next().unwrap().to_owned();
        let chat = format!("{header}\n{{\"request\":{{}}}}\n");
        assert_eq!(
            SpeechCassette::from_jsonl(&chat),
            Err(CassetteError::BadLine { line: 2 })
        );
    }
}
