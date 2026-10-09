//! What a host sends is hostile input: any bytes, in any chunking, end in frames or a typed
//! error, and a header over the cap is refused before its body is waited for.

use model_provider::{ModelName, ProviderError};
use proptest::prelude::*;
use speech_host_client::FrameBuffer;
use speech_provider::{
    AudioMs, HeardText, HostOut, HostVocab, MAX_FRAME_BYTES, SttEnd, encode_frame,
};

/// Every frame the bytes hold, or the error that stopped the reading.
fn read(bytes: &[u8], cuts: &[usize]) -> (Vec<HostOut>, Option<ProviderError>) {
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
    points.extend([0, bytes.len()]);
    points.sort_unstable();
    let mut buffer = FrameBuffer::default();
    let mut frames = Vec::new();
    for w in points.windows(2) {
        buffer.push(&bytes[w[0]..w[1]]);
        loop {
            match buffer.take() {
                Ok(Some(frame)) => frames.push(frame),
                Ok(None) => break,
                Err(error) => return (frames, Some(error)),
            }
        }
    }
    (frames, None)
}

fn done(text: &str) -> HostOut {
    HostOut::Done(SttEnd {
        text: HeardText(text.into()),
        audio: AudioMs(10),
        served: ModelName("m".into()),
    })
}

fn framed(messages: &[HostOut]) -> Vec<u8> {
    messages
        .iter()
        .flat_map(|m| encode_frame(m).unwrap())
        .collect()
}

fn valid() -> Vec<HostOut> {
    vec![
        HostOut::Hello {
            vocab: HostVocab::CURRENT,
            models: vec![],
        },
        done("héllo 世界"),
        HostOut::Failed(ProviderError::Unreachable),
        done(""),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn arbitrary_bytes_end_in_frames_or_a_typed_error(
        bytes in proptest::collection::vec(any::<u8>(), 0..400),
        cuts in proptest::collection::vec(any::<usize>(), 0..8),
    ) {
        let (split, split_error) = read(&bytes, &cuts);
        let (whole, whole_error) = read(&bytes, &[]);
        prop_assert_eq!(split, whole);
        prop_assert_eq!(split_error, whole_error);
    }

    #[test]
    fn valid_frames_read_the_same_whatever_the_split(
        cuts in proptest::collection::vec(any::<usize>(), 0..10),
        noise in proptest::collection::vec(any::<u8>(), 0..24),
        keep in any::<usize>(),
    ) {
        let bytes = framed(&valid());
        let (frames, error) = read(&bytes, &cuts);
        prop_assert_eq!(frames, valid());
        prop_assert_eq!(error, None);
        // Cut short, a stream gives a prefix of the frames and no error; with noise after the
        // cut it gives that prefix and then at most a typed error.
        let mut damaged = bytes[..keep % (bytes.len() + 1)].to_vec();
        damaged.extend(noise);
        let (frames, _) = read(&damaged, &cuts);
        prop_assert!(valid().starts_with(&frames));
    }
}

#[test]
fn a_header_over_the_cap_is_refused_before_the_body_arrives() {
    let mut buffer = FrameBuffer::default();
    buffer.push(&u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes());
    assert!(matches!(buffer.take(), Err(ProviderError::Unreadable(_))));
    let mut buffer = FrameBuffer::default();
    buffer.push(&u32::MAX.to_be_bytes());
    assert!(matches!(buffer.take(), Err(ProviderError::Unreadable(_))));
}

#[test]
fn a_header_at_the_cap_waits_for_its_body() {
    let mut buffer = FrameBuffer::default();
    buffer.push(&u32::try_from(MAX_FRAME_BYTES).unwrap().to_be_bytes());
    assert_eq!(buffer.take(), Ok(None));
}

#[test]
fn a_zero_length_frame_and_a_wrong_shape_are_unreadable() {
    for body in [
        &b""[..],
        b"{}",
        b"[]",
        b"null",
        b"{\"kind\":\"done\"}",
        b"\xff\xfe",
    ] {
        let mut buffer = FrameBuffer::default();
        buffer.push(&u32::try_from(body.len()).unwrap().to_be_bytes());
        buffer.push(body);
        assert!(
            matches!(buffer.take(), Err(ProviderError::Unreadable(_))),
            "{body:?}"
        );
    }
}
