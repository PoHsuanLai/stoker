//! Frames on a stream: reads that survive being cancelled, writes of whole frames.

use model_provider::ProviderError;
use serde::Serialize;
use speech_provider::{FrameError, HostOut, decode_frame, encode_frame, frame_length};
use std::io::ErrorKind;

use tokio::net::UnixStream;

/// The bytes read so far and not yet a whole frame.
#[derive(Debug, Default)]
pub struct FrameReader {
    pending: Vec<u8>,
}

impl FrameReader {
    /// The next host frame. Cancel-safe: bytes read stay buffered, so a dropped call loses none.
    pub async fn next(&mut self, stream: &UnixStream) -> Result<HostOut, ProviderError> {
        loop {
            if let Some(frame) = self.take()? {
                return Ok(frame);
            }
            stream
                .readable()
                .await
                .map_err(|_| ProviderError::Unreachable)?;
            match stream.try_read_buf(&mut self.pending) {
                // The host closed without saying `Done` or `Failed`.
                Ok(0) => return Err(ProviderError::Unreachable),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => return Err(ProviderError::Unreachable),
            }
        }
    }

    fn take(&mut self) -> Result<Option<HostOut>, ProviderError> {
        let Some(header) = self.pending.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = frame_length(*header).map_err(unreadable)?;
        let Some(body) = self.pending.get(4..4 + len) else {
            return Ok(None);
        };
        let frame = decode_frame(body).map_err(unreadable)?;
        self.pending.drain(..4 + len);
        Ok(Some(frame))
    }
}

/// A frame the host sent that cannot be read; the cause is the frame error, never the bytes.
fn unreadable(error: FrameError) -> ProviderError {
    ProviderError::Unreadable(error.to_string())
}

pub async fn write<T: Serialize>(stream: &UnixStream, message: &T) -> Result<(), ProviderError> {
    let frame = encode_frame(message).map_err(|e| ProviderError::BadRequest(e.to_string()))?;
    let mut rest = frame.as_slice();
    while !rest.is_empty() {
        stream
            .writable()
            .await
            .map_err(|_| ProviderError::Unreachable)?;
        match stream.try_write(rest) {
            Ok(n) => rest = rest.get(n..).unwrap_or_default(),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(_) => return Err(ProviderError::Unreachable),
        }
    }
    Ok(())
}
