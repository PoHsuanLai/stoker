//! The speech-host wire in both directions: frames from the host through the client's buffer, and
//! the bodies of both message types decoded directly.
#![no_main]

use libfuzzer_sys::fuzz_target;
use speech_host_client::FrameBuffer;
use speech_provider::{HostIn, HostOut, decode_frame};
use stoker_fuzz::{pieces, split};

fuzz_target!(|data: &[u8]| {
    let (cuts, bytes) = split(data);
    let read = |chunks: &[&[u8]]| {
        let mut buffer = FrameBuffer::default();
        let mut frames = Vec::new();
        for chunk in chunks {
            buffer.push(chunk);
            loop {
                match buffer.take() {
                    Ok(Some(frame)) => frames.push(frame),
                    Ok(None) => break,
                    Err(_) => return (frames, false),
                }
            }
        }
        (frames, true)
    };
    let (split_frames, split_clean) = read(&pieces(bytes, &cuts));
    let (whole_frames, whole_clean) = read(&[bytes]);
    assert_eq!(split_frames, whole_frames);
    assert_eq!(split_clean, whole_clean);
    let _ = decode_frame::<HostIn>(bytes);
    let _ = decode_frame::<HostOut>(bytes);
});
