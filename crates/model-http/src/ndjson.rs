//! A decoder for newline-delimited JSON (Ollama's native stream), fed bytes in any chunking.

/// A stream that is not valid NDJSON framing (a complete line that is not UTF-8, or a line over
/// the limit). Never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LineError {
    #[error("a line is not valid UTF-8")]
    NotUtf8,
    #[error("a line is longer than the decoder accepts")]
    LineTooLong,
}

/// The longest line the decoder buffers.
const LINE_MAX: usize = 4 << 20;

/// Holds the bytes of an incomplete line between `feed` calls. The same bytes give the same
/// lines whatever the chunking; a code point split across chunks is never an error.
#[derive(Debug, Clone, Default)]
pub struct NdjsonDecoder {
    pending: Vec<u8>,
    /// Leading bytes of `pending` known to hold no newline (a long line is scanned once).
    scanned: usize,
}

impl NdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one chunk; returns the complete, non-blank lines it finished (a `\r` before the
    /// `\n` is dropped).
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, LineError> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        let mut used = 0;
        while let Some(rel) = self.pending[self.scanned.max(used)..]
            .iter()
            .position(|b| *b == b'\n')
        {
            let end = self.scanned.max(used) + rel;
            lines.extend(line(&self.pending[used..end])?);
            used = end + 1;
        }
        self.pending.drain(..used);
        self.scanned = self.pending.len();
        if self.pending.len() > LINE_MAX {
            return Err(LineError::LineTooLong);
        }
        Ok(lines)
    }

    /// The stream ended: an unterminated last line, if any (the codec decides whether that is
    /// `Truncated`). A last line that is not UTF-8 is not returned.
    pub fn finish(self) -> Option<String> {
        line(&self.pending).ok().flatten()
    }
}

/// One line without its terminator: `None` when it is blank.
fn line(raw: &[u8]) -> Result<Option<String>, LineError> {
    if raw.len() > LINE_MAX {
        return Err(LineError::LineTooLong);
    }
    let text = std::str::from_utf8(raw).map_err(|_| LineError::NotUtf8)?;
    let text = text.strip_suffix('\r').unwrap_or(text);
    Ok((!text.trim().is_empty()).then(|| text.to_owned()))
}
