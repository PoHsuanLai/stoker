//! What inferd sends the host is input too: any bytes in any chunking end the connection with
//! frames written back or an I/O error, never a panic, and the replies are always whole frames.

use std::io::{self, Cursor, Read, Write};

use model_provider::{ModelName, ProviderError, Support};
use proptest::prelude::*;
use speech_host::{Finished, Recognizer, serve_connection};
use speech_provider::{
    AudioChunk, AudioFormat, AudioMs, HeardText, HostIn, HostOut, HostVocab, LangChoice, LangSet,
    MAX_FRAME_BYTES, PcmBytes, PcmFormat, SampleIndex, SampleRate, SpeechCaps, SpeechIo,
    SpeechModelInfo, SttEnd, SttMode, SttRequest, TranscriptEvent, decode_frame, encode_frame,
};

const FORMAT: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

/// A recognizer that accepts everything and says nothing.
struct Quiet;

impl Recognizer for Quiet {
    fn models(&self) -> Vec<SpeechModelInfo> {
        vec![SpeechModelInfo {
            name: ModelName("m".into()),
            caps: SpeechCaps {
                streaming: Support::Present,
                partials: Support::Absent,
                punctuation: Support::Absent,
                timestamps: Support::Absent,
                langs: LangSet::Any,
                max_audio: AudioMs(1000),
                io: SpeechIo::In { input: FORMAT },
            },
        }]
    }
    fn begin(&mut self, _: &SttRequest) -> Result<(), ProviderError> {
        Ok(())
    }
    fn accept(&mut self, _: &AudioChunk) -> Result<Vec<TranscriptEvent>, ProviderError> {
        Ok(Vec::new())
    }
    fn finish(&mut self) -> Result<Finished, ProviderError> {
        Ok(Finished {
            events: Vec::new(),
            end: SttEnd {
                text: HeardText(String::new()),
                audio: AudioMs(0),
                served: ModelName("m".into()),
            },
        })
    }
    fn reset(&mut self) {}
}

/// Reads its input in the given piece sizes and keeps what is written.
struct Pipe {
    input: Cursor<Vec<u8>>,
    sizes: Vec<usize>,
    next: usize,
    output: Vec<u8>,
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let size = self
            .sizes
            .get(self.next)
            .copied()
            .unwrap_or(usize::MAX)
            .max(1);
        self.next += 1;
        let room = buf.len().min(size);
        self.input.read(&mut buf[..room])
    }
}

impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The replies written for `input`, read back as frames: each must be whole and a `HostOut`.
fn serve(input: Vec<u8>, sizes: Vec<usize>) -> (Vec<HostOut>, bool) {
    let mut pipe = Pipe {
        input: Cursor::new(input),
        sizes,
        next: 0,
        output: Vec::new(),
    };
    let ended = serve_connection(&mut Quiet, &mut pipe).is_ok();
    let mut rest = pipe.output.as_slice();
    let mut replies = Vec::new();
    while !rest.is_empty() {
        let header: [u8; 4] = rest[..4].try_into().expect("a whole header");
        let len = u32::from_be_bytes(header) as usize;
        assert!(len <= MAX_FRAME_BYTES);
        replies.push(decode_frame(&rest[4..4 + len]).expect("a whole HostOut frame"));
        rest = &rest[4 + len..];
    }
    (replies, ended)
}

fn request() -> SttRequest {
    SttRequest {
        model: ModelName("m".into()),
        mode: SttMode::Streaming {
            chunk: AudioMs(560),
        },
        lang: LangChoice::Auto,
        format: FORMAT,
    }
}

fn conversation() -> Vec<u8> {
    let chunk = AudioChunk {
        format: FORMAT,
        at: SampleIndex(0),
        pcm: PcmBytes::new(vec![1; 64]),
    };
    [
        HostIn::Hello {
            vocab: HostVocab::CURRENT,
        },
        HostIn::Begin(request()),
        HostIn::Audio(chunk),
        HostIn::End,
        HostIn::Cancel,
    ]
    .iter()
    .flat_map(|m| encode_frame(m).unwrap())
    .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn arbitrary_bytes_never_panic_and_replies_are_whole_frames(
        bytes in proptest::collection::vec(any::<u8>(), 0..300),
        sizes in proptest::collection::vec(1usize..40, 0..10),
    ) {
        let (split, _) = serve(bytes.clone(), sizes);
        let (whole, _) = serve(bytes, Vec::new());
        prop_assert_eq!(split, whole);
    }

    #[test]
    fn a_valid_conversation_answers_the_same_whatever_the_read_sizes(
        sizes in proptest::collection::vec(1usize..64, 0..20),
        keep in any::<usize>(),
        noise in proptest::collection::vec(any::<u8>(), 0..16),
    ) {
        let (replies, ended) = serve(conversation(), sizes.clone());
        prop_assert!(ended);
        prop_assert_eq!(replies.len(), 2);
        let mut damaged = conversation();
        damaged.truncate(keep % (damaged.len() + 1));
        damaged.extend(noise);
        let (split, _) = serve(damaged.clone(), sizes);
        let (whole, _) = serve(damaged, Vec::new());
        prop_assert_eq!(split, whole);
    }
}

#[test]
fn a_header_over_the_cap_ends_the_connection_with_nothing_read_after_it() {
    let mut input = u32::try_from(MAX_FRAME_BYTES + 1)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    input.extend(vec![0u8; 16]);
    let (replies, ended) = serve(input, Vec::new());
    assert!(replies.is_empty());
    assert!(!ended);
}

#[test]
fn a_frame_that_is_not_a_host_message_is_refused_and_the_connection_goes_on() {
    let mut input = Vec::new();
    for body in [
        &b"{}"[..],
        b"null",
        b"\xff",
        b"{\"kind\":\"audio\",\"v\":{}}",
    ] {
        input.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
        input.extend(body);
    }
    input.extend(
        encode_frame(&HostIn::Hello {
            vocab: HostVocab::CURRENT,
        })
        .unwrap(),
    );
    let (replies, ended) = serve(input, Vec::new());
    assert!(ended);
    assert_eq!(replies.len(), 5);
    assert!(matches!(replies[4], HostOut::Hello { .. }));
    assert!(
        replies[..4]
            .iter()
            .all(|r| matches!(r, HostOut::Failed(ProviderError::BadRequest(_))))
    );
}

#[test]
fn a_stream_cut_inside_a_header_or_a_body_is_an_error_not_a_reply() {
    let whole = conversation();
    for cut in [1, 3, 5, 9] {
        let (replies, ended) = serve(whole[..cut].to_vec(), Vec::new());
        assert!(replies.is_empty());
        assert!(!ended, "cut {cut}");
    }
}
