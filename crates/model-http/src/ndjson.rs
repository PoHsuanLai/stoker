//! A decoder for newline-delimited JSON (Ollama's native stream), fed bytes in any chunking.

/// A stream that is not valid NDJSON framing (a complete line that is not UTF-8, or a line over
/// the limit). Never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LineError {
    #[error("a line is not valid UTF-8")]
    NotUtf8,
    #[error("a line is longer than the decoder accepts")]
    LineTooLong,
}

/// Holds the bytes of an incomplete line between `feed` calls. The same bytes give the same
/// lines whatever the chunking; a code point split across chunks is never an error.
#[derive(Debug, Clone, Default)]
pub struct NdjsonDecoder {
    pending: Vec<u8>,
}

impl NdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one chunk; returns the complete, non-blank lines it finished.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, LineError> {
        let _ = (bytes, &self.pending);
        todo!("NdjsonDecoder::feed: split on newline, skip blank lines, cap the line")
    }

    /// The stream ended: an unterminated last line, if any (the codec decides whether that is
    /// `Truncated`).
    pub fn finish(self) -> Option<String> {
        let _ = self.pending;
        todo!("NdjsonDecoder::finish")
    }
}
