//! The pure half of the framing: bytes in any chunking in, whole host frames out.

use model_provider::ProviderError;
use speech_provider::{FrameError, HostOut, decode_frame, frame_length};

/// The bytes read so far and not yet a whole frame; the pure half of the reader, so any bytes in
/// any chunking can be fed to it without a socket.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    pub(crate) pending: Vec<u8>,
}

impl FrameBuffer {
    /// Adds bytes read from the host.
    pub fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
    }

    /// The next whole host frame, `None` while one is incomplete. A header over the cap is
    /// refused before the body is waited for; a body that is not a host message is unreadable.
    pub fn take(&mut self) -> Result<Option<HostOut>, ProviderError> {
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
