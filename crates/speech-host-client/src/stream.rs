//! Frames on a stream: reads that survive being cancelled, writes of whole frames.

use std::io::ErrorKind;

use model_provider::ProviderError;
use serde::Serialize;
use speech_provider::{HostOut, encode_frame};
use tokio::net::UnixStream;

use crate::framed::FrameBuffer;

/// Reads host frames from a stream. Cancel-safe: what was read stays buffered.
#[derive(Debug, Default)]
pub struct FrameReader {
    buffer: FrameBuffer,
}

impl FrameReader {
    /// The next host frame. Cancel-safe: bytes read stay buffered, so a dropped call loses none.
    pub async fn next(&mut self, stream: &UnixStream) -> Result<HostOut, ProviderError> {
        loop {
            if let Some(frame) = self.buffer.take()? {
                return Ok(frame);
            }
            stream
                .readable()
                .await
                .map_err(|_| ProviderError::Unreachable)?;
            match stream.try_read_buf(&mut self.buffer.pending) {
                // The host closed without saying `Done` or `Failed`.
                Ok(0) => return Err(ProviderError::Unreachable),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => return Err(ProviderError::Unreachable),
            }
        }
    }
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
