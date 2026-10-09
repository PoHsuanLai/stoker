//! `OpenAiSpeech` speech against the fake server (`speech_fake`): the streamed PCM in whole
//! samples under any cutting, barge-in, and what every failure becomes. No GPU, no network.

use crate::speech_fake;

use std::sync::atomic::Ordering;
use std::time::Duration;

use model_openai_compat::{KOKORO_FORMAT, SpeechFlavor, encode_speech_request};
use model_provider::{ModelName, ProviderError, ServerStatus};
use proptest::prelude::*;
use speech_fake::*;
use speech_provider::{AudioMs, SampleIndex, TextToSpeech};

// ---- speech --------------------------------------------------------------------------------

/// `n` seconds-worth of distinct samples at 24 kHz: sample `i` is `i` mod 65536.
fn tone(samples: u32) -> Vec<u8> {
    (0..samples)
        .flat_map(|i| (i as u16).to_le_bytes())
        .collect()
}

fn cut(bytes: &[u8], sizes: &[usize]) -> Vec<Vec<u8>> {
    let mut rest = bytes;
    let mut pieces = Vec::new();
    for size in sizes.iter().cycle() {
        if rest.is_empty() {
            return pieces;
        }
        let (head, tail) = rest.split_at((*size).clamp(1, rest.len()));
        pieces.push(head.to_vec());
        rest = tail;
    }
    pieces
}

#[tokio::test]
async fn speech_streams_pcm_into_the_sink_in_whole_samples() {
    let audio = tone(2_400);
    // Cut at odd places: the decoder carries the half sample.
    let pieces = cut(&audio, &[1_001, 777, 3_000]);
    let server = fake(move |_| pcm(pieces.clone(), 1));
    let mut speaker = Speaker::default();
    let end = server
        .speech(SpeechFlavor::KokoroFastApi)
        .speak(&tts(), &mut speaker)
        .await
        .unwrap();
    let sent = &server.requests()[0];
    assert_eq!(
        (sent.method.as_str(), sent.path.as_str()),
        ("POST", "/v1/audio/speech")
    );
    assert_eq!(
        sent.body,
        encode_speech_request(&tts(), SpeechFlavor::KokoroFastApi)
            .unwrap()
            .0
            .into_bytes()
    );
    // The same samples, in order, each chunk numbered from the start, all whole.
    let heard: Vec<u8> = speaker
        .chunks
        .iter()
        .flat_map(|c| c.pcm.as_slice().to_vec())
        .collect();
    assert_eq!(heard, audio);
    let mut at = 0;
    for c in &speaker.chunks {
        assert_eq!(
            (c.format, c.at, c.pcm.len() % 2),
            (KOKORO_FORMAT, SampleIndex(at), 0)
        );
        at += u64::from(c.samples());
    }
    assert_eq!(
        (end.audio, end.served),
        (AudioMs(100), ModelName("kokoro".into()))
    );
}

#[tokio::test]
async fn barge_in_stops_the_stream_and_answers_the_audio_played_so_far() {
    let pieces: Vec<Vec<u8>> = (0..200).map(|_| vec![0u8; 2_400]).collect();
    let server = fake(move |_| pcm(pieces.clone(), 5));
    let mut speaker = Speaker {
        stop_after: Some(2),
        ..Speaker::default()
    };
    let end = server
        .speech(SpeechFlavor::KokoroFastApi)
        .speak(&tts(), &mut speaker)
        .await
        .unwrap();
    assert_eq!(speaker.chunks.len(), 2);
    assert_eq!(end.audio, AudioMs(100));
    // The connection was closed, so the server's long stream hit a broken pipe.
    for _ in 0..100 {
        if server.peer_closed.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the server was still streaming");
}

#[tokio::test]
async fn a_refused_speech_request_is_the_error_and_plays_nothing() {
    let cases: Vec<(Reply, ProviderError)> = vec![
        (
            json("400 Bad Request", r#"{"detail":"SECRET-BODY"}"#),
            ProviderError::BadRequest("http_400".into()),
        ),
        (
            json("503 Service Unavailable", "SECRET-BODY"),
            ProviderError::Server(ServerStatus(503)),
        ),
        (
            Reply {
                content_type: "text/html",
                ..json("200 OK", "<html>SECRET-BODY</html>")
            },
            ProviderError::Unreadable("an HTML page was served instead of a reply".into()),
        ),
        (
            pcm(vec![], 0),
            ProviderError::Unreadable("the reply held no audio".into()),
        ),
    ];
    for (reply, want) in cases {
        let server = fake(move |_| reply.clone());
        let mut speaker = Speaker::default();
        let got = server
            .speech(SpeechFlavor::KokoroFastApi)
            .speak(&tts(), &mut speaker)
            .await
            .unwrap_err();
        assert_eq!(got, want);
        assert!(speaker.chunks.is_empty());
        assert!(!format!("{got:?}").contains("SECRET"));
    }
}

#[tokio::test]
async fn a_format_the_server_does_not_stream_is_refused_before_sending() {
    let server = fake(|_| pcm(vec![vec![0, 0]], 0));
    let mut request = tts();
    request.format = S16_16K;
    let got = server
        .speech(SpeechFlavor::KokoroFastApi)
        .speak(&request, &mut Speaker::default())
        .await;
    assert_eq!(
        got.unwrap_err(),
        ProviderError::BadRequest("this server cannot take the audio format".into())
    );
    assert!(server.requests().is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// However the engine cuts its stream (and the transport reads it), the sink gets the same
    /// samples and the same length.
    #[test]
    fn the_samples_do_not_depend_on_how_the_stream_is_cut(
        samples in 1_u32..3_000,
        sizes in proptest::collection::vec(1_usize..2_500, 1..6),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let audio = tone(samples);
        let pieces = cut(&audio, &sizes);
        let (heard, ms) = runtime.block_on(async {
            let server = fake(move |_| pcm(pieces.clone(), 0));
            let mut speaker = Speaker::default();
            let end = server.speech(SpeechFlavor::KokoroFastApi).speak(&tts(), &mut speaker).await.unwrap();
            let heard: Vec<u8> = speaker.chunks.iter().flat_map(|c| c.pcm.as_slice().to_vec()).collect();
            (heard, end.audio)
        });
        prop_assert_eq!(heard, audio);
        prop_assert_eq!(u64::from(ms.0), u64::from(samples) * 1000 / 24_000);
    }
}
