//! `OpenAiSpeech` transcription against the fake server (`speech_fake`): the upload it sends, the
//! event it delivers, and what every failure becomes. No GPU, no network.

use crate::speech_fake;

use model_http::{HttpClient, HttpTarget, WaitMs};
use model_openai_compat::{OpenAiSpeech, SpeechFlavor, encode_transcription};
use model_provider::{ProviderError, RetrySeconds, ServerStatus};
use speech_fake::*;
use speech_provider::{
    AudioMs, HeardText, Lang, LangChoice, SampleIndex, SpeechToText, TextToSpeech, TranscriptEvent,
};
use std::collections::VecDeque;

// ---- transcription -------------------------------------------------------------------------

#[tokio::test]
async fn a_transcription_uploads_the_wav_and_answers_one_final_event() {
    let server = fake(|_| json("200 OK", r#"{"text":" hello world"}"#));
    let audio = vec![chunk(0, 160), chunk(160, 80)];
    let request = stt(LangChoice::Prefer(vec![Lang::new("en-US").unwrap()]));
    let mut events = Events::default();
    let end = server
        .speech(SpeechFlavor::Vllm)
        .transcribe(&request, &mut Spoken(audio.clone().into()), &mut events)
        .await
        .unwrap();
    // The upload is the codec's body, byte for byte, at the documented path.
    let sent = &server.requests()[0];
    let want = encode_transcription(&request, &audio).unwrap();
    assert_eq!(
        (sent.method.as_str(), sent.path.as_str()),
        ("POST", "/v1/audio/transcriptions")
    );
    assert!(sent.head.to_ascii_lowercase().contains(&format!(
        "content-type: {}",
        want.content_type.to_ascii_lowercase()
    )));
    assert_eq!(sent.body, want.bytes);
    // One final event, spanning the audio; the end names the model that was asked.
    assert_eq!(
        events.0,
        [TranscriptEvent::Final {
            text: HeardText(" hello world".into()),
            from: SampleIndex(0),
            to: SampleIndex(240),
        }]
    );
    assert_eq!(end.text, HeardText(" hello world".into()));
    assert_eq!(end.audio, AudioMs(15));
    assert_eq!(end.served, request.model);
}

#[tokio::test]
async fn no_audio_sends_nothing_and_says_nothing() {
    let server = fake(|_| json("200 OK", r#"{"text":"x"}"#));
    let mut events = Events::default();
    let end = server
        .speech(SpeechFlavor::Vllm)
        .transcribe(
            &stt(LangChoice::Auto),
            &mut Spoken(VecDeque::new()),
            &mut events,
        )
        .await
        .unwrap();
    assert!(server.requests().is_empty());
    assert_eq!(
        (end.text, end.audio),
        (HeardText(String::new()), AudioMs(0))
    );
    assert_eq!(events.0.len(), 1);
}

#[tokio::test]
async fn every_failure_of_a_transcription_is_the_error_it_means_and_names_no_text() {
    let cases: Vec<(Reply, ProviderError)> = vec![
        (
            json("401 Unauthorized", r#"{"error":"SECRET-BODY"}"#),
            ProviderError::Unauthorized,
        ),
        (json("403 Forbidden", "{}"), ProviderError::Unauthorized),
        (
            Reply {
                retry_after: Some(7),
                ..json("429 Too Many Requests", "{}")
            },
            ProviderError::RateLimited(RetrySeconds(7)),
        ),
        (
            json("500 Internal Server Error", "SECRET-BODY"),
            ProviderError::Server(ServerStatus(500)),
        ),
        (
            json("404 Not Found", "SECRET-BODY"),
            ProviderError::BadRequest("http_404".into()),
        ),
        (
            json("200 OK", "not json"),
            ProviderError::Unreadable("the transcription is not the documented JSON".into()),
        ),
        (
            json("200 OK", r#"{"words":[]}"#),
            ProviderError::Unreadable("the transcription is not the documented JSON".into()),
        ),
        (
            Reply {
                content_type: "text/html",
                ..json("200 OK", "<html>SECRET-BODY</html>")
            },
            ProviderError::Unreadable("an HTML page was served instead of a reply".into()),
        ),
    ];
    for (reply, want) in cases {
        let server = fake(move |_| reply.clone());
        let got = server
            .speech(SpeechFlavor::Vllm)
            .transcribe(
                &stt(LangChoice::Auto),
                &mut Spoken(vec![chunk(0, 16)].into()),
                &mut Events::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(got, want);
        assert!(!format!("{got:?}").contains("SECRET"), "{got:?}");
    }
}

#[tokio::test]
async fn nobody_listening_is_unreachable_and_a_silent_server_times_out() {
    let nowhere = OpenAiSpeech::new(
        HttpClient::new(endpoint(HttpTarget::Unix(scratch()))),
        SpeechFlavor::Vllm,
    );
    let got = nowhere
        .transcribe(
            &stt(LangChoice::Auto),
            &mut Spoken(vec![chunk(0, 16)].into()),
            &mut Events::default(),
        )
        .await;
    assert_eq!(got.unwrap_err(), ProviderError::Unreachable);
    let server = fake(|_| Reply {
        gap_ms: 400,
        ..json("200 OK", "{}")
    });
    let mut slow = server.endpoint.clone();
    slow.timeouts.first_byte = WaitMs(50);
    slow.timeouts.idle = WaitMs(50);
    let speech = OpenAiSpeech::new(HttpClient::new(slow), SpeechFlavor::Vllm);
    let got = speech
        .transcribe(
            &stt(LangChoice::Auto),
            &mut Spoken(vec![chunk(0, 16)].into()),
            &mut Events::default(),
        )
        .await;
    assert!(
        matches!(
            got,
            Err(ProviderError::Timeout | ProviderError::Unreachable)
        ),
        "{got:?}"
    );
}

#[tokio::test]
async fn a_server_has_one_direction() {
    let server = fake(|_| json("200 OK", "{}"));
    let vllm = server.speech(SpeechFlavor::Vllm);
    let kokoro = server.speech(SpeechFlavor::KokoroFastApi);
    let no = |what: &str| ProviderError::BadRequest(format!("this server has no {what} endpoint"));
    assert_eq!(
        TextToSpeech::describe(&vllm).await.unwrap_err(),
        no("speech")
    );
    assert_eq!(
        vllm.speak(&tts(), &mut Speaker::default())
            .await
            .unwrap_err(),
        ProviderError::BadRequest("this server cannot take the audio format".into())
    );
    assert_eq!(
        SpeechToText::describe(&kokoro).await.unwrap_err(),
        no("transcription")
    );
    let got = kokoro
        .transcribe(
            &stt(LangChoice::Auto),
            &mut Spoken(vec![chunk(0, 16)].into()),
            &mut Events::default(),
        )
        .await;
    assert_eq!(got.unwrap_err(), no("transcription"));
    assert!(server.requests().is_empty());
}
