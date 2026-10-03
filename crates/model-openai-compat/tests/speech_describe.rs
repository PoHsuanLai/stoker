//! `OpenAiSpeech::describe` against the fake server (`speech_fake`): the served models the
//! catalog knows, the voices read from the server, and an unreadable listing. No GPU, no network.

mod speech_fake;

use model_openai_compat::{KOKORO_FORMAT, SpeechFlavor};
use model_provider::{ModelName, ProviderError, ServerStatus, Support};
use speech_fake::*;
use speech_provider::{
    AudioMs, LangSet, SpeechCaps, SpeechIo, SpeechModelInfo, SpeechToText, TextToSpeech, VoiceId,
};

// ---- describe ------------------------------------------------------------------------------

fn info(name: &str, io: SpeechIo) -> SpeechModelInfo {
    SpeechModelInfo {
        name: ModelName(name.into()),
        caps: SpeechCaps {
            streaming: Support::Absent,
            partials: Support::Absent,
            punctuation: Support::Present,
            timestamps: Support::Absent,
            langs: LangSet::Any,
            max_audio: AudioMs(30_000),
            io,
        },
    }
}

fn voices(names: &[&str]) -> Vec<VoiceId> {
    names.iter().map(|n| VoiceId::new(*n).unwrap()).collect()
}

#[tokio::test]
async fn describe_in_answers_the_listed_models_the_catalog_knows() {
    let server = fake(|seen| {
        assert_eq!(
            (seen.method.as_str(), seen.path.as_str()),
            ("GET", "/v1/models")
        );
        json(
            "200 OK",
            r#"{"object":"list","data":[{"id":"openai/whisper-large-v3"},{"id":"unknown/other"}]}"#,
        )
    });
    let whisper = info("openai/whisper-large-v3", SpeechIo::In { input: S16_16K });
    let absent = info("not/served", SpeechIo::In { input: S16_16K });
    let kokoro = info(
        "kokoro",
        SpeechIo::Out {
            output: KOKORO_FORMAT,
            voices: vec![],
        },
    );
    let speech =
        server
            .speech(SpeechFlavor::Vllm)
            .with_known(vec![whisper.clone(), absent, kokoro]);
    // Only what is served, only the right direction; the unknown served model has no caps to say.
    assert_eq!(
        SpeechToText::describe(&speech).await.unwrap(),
        vec![whisper]
    );
    assert_eq!(server.requests().len(), 1);
    // Without the catalog nothing can be described.
    let bare = server.speech(SpeechFlavor::Vllm);
    assert_eq!(SpeechToText::describe(&bare).await.unwrap(), vec![]);
}

#[tokio::test]
async fn describe_out_reads_the_voices_from_the_server() {
    for voice_body in [
        r#"{"voices":["af_heart","bf_emma","not a voice!"]}"#,
        r#"["af_heart","bf_emma"]"#,
    ] {
        let server = fake(move |seen| match seen.path.as_str() {
            "/v1/models" => json("200 OK", r#"{"data":[{"id":"kokoro"},{"id":"tts-1"}]}"#),
            "/v1/audio/voices" => json("200 OK", voice_body),
            other => panic!("{other}"),
        });
        let catalog = info(
            "kokoro",
            SpeechIo::Out {
                output: KOKORO_FORMAT,
                voices: voices(&["zf_xiaoxiao"]),
            },
        );
        let speech = server
            .speech(SpeechFlavor::KokoroFastApi)
            .with_known(vec![catalog.clone()]);
        let got = TextToSpeech::describe(&speech).await.unwrap();
        let want = info(
            "kokoro",
            SpeechIo::Out {
                output: KOKORO_FORMAT,
                voices: voices(&["af_heart", "bf_emma"]),
            },
        );
        assert_eq!(got, vec![want], "{voice_body}");
    }
    // A server that lists no voices leaves the catalog's list.
    let server = fake(|seen| match seen.path.as_str() {
        "/v1/models" => json("200 OK", r#"{"data":[{"id":"kokoro"}]}"#),
        _ => json("200 OK", r#"{"voices":[]}"#),
    });
    let catalog = info(
        "kokoro",
        SpeechIo::Out {
            output: KOKORO_FORMAT,
            voices: voices(&["zf_xiaoxiao"]),
        },
    );
    let speech = server
        .speech(SpeechFlavor::KokoroFastApi)
        .with_known(vec![catalog.clone()]);
    assert_eq!(
        TextToSpeech::describe(&speech).await.unwrap(),
        vec![catalog]
    );
}

#[tokio::test]
async fn an_unreadable_listing_is_unreadable_and_a_refusal_is_its_error() {
    let bad = |body: &'static str, status: &'static str| {
        let server = fake(move |_| json(status, body));
        let speech = server.speech(SpeechFlavor::Vllm);
        async move {
            let got = SpeechToText::describe(&speech).await.unwrap_err();
            // The server lives until the answer is in.
            drop(server);
            got
        }
    };
    for body in [
        "nope",
        "{}",
        r#"{"data":[{"name":"x"}]}"#,
        r#"{"data":[{"id":""}]}"#,
    ] {
        assert!(
            matches!(bad(body, "200 OK").await, ProviderError::Unreadable(_)),
            "{body}"
        );
    }
    assert_eq!(
        bad("{}", "401 Unauthorized").await,
        ProviderError::Unauthorized
    );
    assert_eq!(
        bad("{}", "502 Bad Gateway").await,
        ProviderError::Server(ServerStatus(502))
    );
}
