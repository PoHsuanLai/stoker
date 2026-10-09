//! The pure speech codec: the speech request, the transcription upload (a WAV file in a
//! multipart body), the transcription reply and the streamed PCM.

use model_openai_compat::{
    AudioCodecError, KOKORO_FORMAT, PcmDecoder, SpeechFlavor, decode_transcription,
    encode_speech_request, encode_transcription,
};
use model_provider::ModelName;
use proptest::prelude::*;
use speech_provider::{
    AudioChunk, AudioFormat, Lang, LangChoice, PcmBytes, PcmFormat, SampleIndex, SampleRate,
    SpokenText, SttMode, SttRequest, TtsRequest, VoiceId,
};

const S16_16K: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

fn tts(format: AudioFormat) -> TtsRequest {
    TtsRequest {
        model: ModelName("kokoro".into()),
        text: SpokenText::new("Hello \"world\"").unwrap(),
        voice: VoiceId::new("af_heart").unwrap(),
        lang: Lang::new("en-US").unwrap(),
        format,
    }
}

#[test]
fn the_speech_request_is_the_documented_body() {
    let body = encode_speech_request(&tts(KOKORO_FORMAT), SpeechFlavor::KokoroFastApi).unwrap();
    let value: serde_json::Value = serde_json::from_str(&body.0).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"model":"kokoro","input":"Hello \"world\"","voice":"af_heart",
            "response_format":"pcm","stream":true})
    );
    // A server without the endpoint, or a format the endpoint does not stream.
    assert_eq!(
        encode_speech_request(&tts(KOKORO_FORMAT), SpeechFlavor::Vllm),
        Err(AudioCodecError::UnsupportedFormat)
    );
    assert_eq!(
        encode_speech_request(&tts(S16_16K), SpeechFlavor::KokoroFastApi),
        Err(AudioCodecError::UnsupportedFormat)
    );
}

fn stt(lang: LangChoice) -> SttRequest {
    SttRequest {
        model: ModelName("whisper".into()),
        mode: SttMode::Batch,
        lang,
        format: S16_16K,
    }
}

fn chunk(format: AudioFormat, at: u64, pcm: Vec<u8>) -> AudioChunk {
    AudioChunk {
        format,
        at: SampleIndex(at),
        pcm: PcmBytes::new(pcm),
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

/// The file part of a multipart body, and the form fields before it.
fn parts(body: &model_openai_compat::MultipartBody) -> (Vec<(String, String)>, Vec<u8>) {
    let boundary = body
        .content_type
        .split("boundary=")
        .nth(1)
        .unwrap()
        .to_owned();
    let marker = format!("--{boundary}");
    let text = &body.bytes;
    let file_at = find(text, b"name=\"file\"").unwrap();
    let head = String::from_utf8_lossy(&text[..file_at]).into_owned();
    let fields = head
        .split(&marker)
        .filter_map(|part| {
            let name = part.split("name=\"").nth(1)?.split('"').next()?.to_owned();
            let value = part
                .split("\r\n\r\n")
                .nth(1)?
                .trim_end_matches("\r\n")
                .to_owned();
            Some((name, value))
        })
        .collect();
    let data_at = file_at + find(&text[file_at..], b"\r\n\r\n").unwrap() + 4;
    let end = text.len() - format!("\r\n{marker}--\r\n").len();
    assert!(text.ends_with(format!("\r\n{marker}--\r\n").as_bytes()));
    (fields, text[data_at..end].to_vec())
}

#[test]
fn a_transcription_upload_is_a_wav_file_in_a_multipart_body() {
    let samples: Vec<u8> = (0..100_i16).flat_map(i16::to_le_bytes).collect();
    let audio = [
        chunk(S16_16K, 0, samples[..120].to_vec()),
        chunk(S16_16K, 60, samples[120..].to_vec()),
    ];
    let prefer = LangChoice::Prefer(vec![
        Lang::new("zh-CN").unwrap(),
        Lang::new("en-US").unwrap(),
    ]);
    let body = encode_transcription(&stt(prefer), &audio).unwrap();
    assert!(
        body.content_type
            .starts_with("multipart/form-data; boundary=")
    );
    let (fields, wav) = parts(&body);
    assert_eq!(
        fields,
        [
            ("model".to_owned(), "whisper".to_owned()),
            ("response_format".to_owned(), "json".to_owned()),
            ("language".to_owned(), "zh".to_owned()),
        ]
    );
    // The 44-byte header, then the samples, in order.
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(u32_at(&wav, 4) as usize, wav.len() - 8);
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(&wav, 16), 16);
    assert_eq!((u16_at(&wav, 20), u16_at(&wav, 22)), (1, 1), "PCM, mono");
    assert_eq!(u32_at(&wav, 24), 16_000);
    assert_eq!(u32_at(&wav, 28), 32_000, "bytes per second");
    assert_eq!((u16_at(&wav, 32), u16_at(&wav, 34)), (2, 16));
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32_at(&wav, 40), 200);
    assert_eq!(&wav[44..], &samples[..]);
    // `Auto` leaves the language out, and the audio never prints.
    let auto = encode_transcription(&stt(LangChoice::Auto), &audio).unwrap();
    assert!(parts(&auto).0.iter().all(|(name, _)| name != "language"));
    assert!(!format!("{auto:?}").contains("RIFF"));
}

#[test]
fn a_float_file_says_so_a_partial_sample_is_cut_and_mixed_formats_are_refused() {
    let float = AudioFormat {
        rate: SampleRate(24_000),
        pcm: PcmFormat::F32Le,
    };
    let body = encode_transcription(
        &stt(LangChoice::Auto),
        &[chunk(float, 0, vec![0, 0, 0x80, 0x3f, 9, 9])],
    )
    .unwrap();
    let (_, wav) = parts(&body);
    assert_eq!((u16_at(&wav, 20), u16_at(&wav, 34)), (3, 32));
    assert_eq!(u32_at(&wav, 28), 96_000);
    assert_eq!(
        &wav[44..],
        &[0, 0, 0x80, 0x3f],
        "the two stray bytes are not a sample"
    );
    assert_eq!(
        encode_transcription(
            &stt(LangChoice::Auto),
            &[chunk(S16_16K, 0, vec![0, 0]), chunk(float, 1, vec![0; 4])]
        ),
        Err(AudioCodecError::MixedFormats)
    );
    // No audio is an empty but valid file.
    let empty = encode_transcription(&stt(LangChoice::Auto), &[]).unwrap();
    assert_eq!(parts(&empty).1.len(), 44);
}

#[test]
fn the_boundary_never_appears_in_the_audio() {
    let hostile = b"------stoker0f3c9a1e".to_vec();
    let body = encode_transcription(
        &stt(LangChoice::Auto),
        &[chunk(S16_16K, 0, hostile.clone())],
    )
    .unwrap();
    let boundary = body.content_type.split("boundary=").nth(1).unwrap();
    assert_ne!(boundary, "----stoker0f3c9a1e");
    let (_, wav) = parts(&body);
    assert_eq!(&wav[44..], &hostile[..]);
}

#[test]
fn a_transcription_reply_is_its_text_field() {
    assert_eq!(
        decode_transcription(br#"{"text":" hello there"}"#)
            .unwrap()
            .0,
        " hello there"
    );
    assert_eq!(
        decode_transcription(br#"{"text":"","usage":{"seconds":3}}"#)
            .unwrap()
            .0,
        ""
    );
    for bad in [
        &b"nope"[..],
        br#"{"txt":"x"}"#,
        br#"{"text":5}"#,
        br#"["text"]"#,
        b"",
    ] {
        assert_eq!(decode_transcription(bad), Err(AudioCodecError::Unreadable));
    }
}

#[test]
fn pcm_is_cut_at_sample_boundaries_and_numbered() {
    let mut decoder = PcmDecoder::new(KOKORO_FORMAT);
    assert_eq!(decoder.feed(&[1]), None, "half a sample waits");
    let first = decoder.feed(&[0, 2, 0, 3]).unwrap();
    assert_eq!(
        (first.at, first.pcm.as_slice()),
        (SampleIndex(0), &[1, 0, 2, 0][..])
    );
    let second = decoder.feed(&[0, 4, 0]).unwrap();
    assert_eq!(
        (second.at, second.pcm.as_slice()),
        (SampleIndex(2), &[3, 0, 4, 0][..])
    );
    assert_eq!(first.format, KOKORO_FORMAT);
    assert_eq!(decoder.feed(&[]), None);
    // A float format joins four bytes.
    let mut floats = PcmDecoder::new(AudioFormat {
        rate: SampleRate(24_000),
        pcm: PcmFormat::F32Le,
    });
    assert_eq!(floats.feed(&[0; 3]), None);
    assert_eq!(floats.feed(&[0; 6]).unwrap().pcm.len(), 8);
}

proptest! {
    #[test]
    fn pcm_chunking_does_not_change_the_samples_or_their_numbers(
        bytes in proptest::collection::vec(any::<u8>(), 0..400),
        cuts in proptest::collection::vec(any::<usize>(), 0..8),
    ) {
        let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
        points.sort_unstable();
        let mut decoder = PcmDecoder::new(KOKORO_FORMAT);
        let mut joined = Vec::new();
        let mut next = 0_u64;
        let mut start = 0;
        for end in points.into_iter().chain([bytes.len()]) {
            if let Some(c) = decoder.feed(&bytes[start..end]) {
                prop_assert_eq!(c.at, SampleIndex(next));
                prop_assert_eq!(c.pcm.len() % 2, 0);
                next += (c.pcm.len() / 2) as u64;
                joined.extend_from_slice(c.pcm.as_slice());
            }
            start = end;
        }
        prop_assert_eq!(&joined[..], &bytes[..bytes.len() - bytes.len() % 2]);
    }
}
