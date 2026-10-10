//! A decoder for server-sent events, fed bytes in any chunking.
//!
// Portions adapted from rig (https://github.com/0xPlaygrounds/rig, crates/rig-core, commit acdcf34),
// MIT License, Copyright (c) 2024, Playgrounds Analytics Inc. See THIRD-PARTY-NOTICES.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventName(pub String);

/// One event: its optional name and its data lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<EventName>,
    pub data: String,
}

/// A stream that is not valid SSE (not UTF-8, or a line over the limit). Never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SseError {
    #[error("the stream is not valid UTF-8")]
    NotUtf8,
    #[error("a line is longer than the decoder accepts")]
    LineTooLong,
}

/// The longest line the decoder buffers. A model's chunk is a few hundred bytes; a line this long
/// is a broken or hostile stream.
const LINE_MAX: usize = 1 << 20;

/// The most data one event may hold (the sum of its `data:` lines), so a stream cannot grow an
/// event without end. Over it is `LineTooLong`.
const EVENT_DATA_MAX: usize = 8 << 20;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Whether the first bytes of the stream have been looked at (a byte order mark is dropped there).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Start {
    #[default]
    Fresh,
    Begun,
}

/// Whether more bytes may follow: a final `\r` is a line end only once the stream is over,
/// because the next byte may be the `\n` of a CRLF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Open,
    Over,
}

/// Holds the bytes of an incomplete line between `feed` calls. The same bytes give the same
/// events whatever the chunking.
///
/// Line ends are `\n`, `\r` and `\r\n`; a leading byte order mark is dropped; lines starting with
/// `:` are comments; `data` lines join with `\n`; one space after the colon is dropped; a blank
/// line dispatches the event if it has data and otherwise only resets the event name; `id` and
/// `retry` are not kept. Only complete lines are decoded, so a code point split across chunks is
/// never an error.
#[derive(Debug, Clone, Default)]
pub struct SseDecoder {
    start: Start,
    pending: Vec<u8>,
    /// How many leading bytes of `pending` are known to hold no line end, so a line fed one byte
    /// at a time is scanned once and not once per byte.
    scanned: usize,
    event: Option<EventName>,
    data: Vec<String>,
    data_len: usize,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one chunk; returns the events it completed.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        self.pending.extend_from_slice(bytes);
        self.drain(Stream::Open)
    }

    /// The stream ended: an event left half-built is dropped, as the SSE standard says. The only
    /// event this can return is one whose blank line was a final `\r`.
    pub fn finish(mut self) -> Vec<SseEvent> {
        self.drain(Stream::Over).unwrap_or_default()
    }

    fn drain(&mut self, stream: Stream) -> Result<Vec<SseEvent>, SseError> {
        if self.start == Start::Fresh {
            let waiting = BOM.starts_with(&self.pending) && stream == Stream::Open;
            if waiting {
                return Ok(Vec::new());
            }
            if self.pending.starts_with(BOM) {
                self.pending.drain(..BOM.len());
            }
            self.start = Start::Begun;
        }
        let mut events = Vec::new();
        // Lines are cut by offset and the consumed prefix is dropped once, so a chunk of many
        // short lines costs linear time.
        let mut used = 0;
        while let Some((end, next)) = line_end(&self.pending, used, self.scanned.max(used), stream)
        {
            let line =
                std::str::from_utf8(&self.pending[used..end]).map_err(|_| SseError::NotUtf8)?;
            let event = Self::absorb(&mut self.event, &mut self.data, &mut self.data_len, line)?;
            events.extend(event);
            used = next;
        }
        self.pending.drain(..used);
        self.scanned = match self.pending.last() {
            Some(b'\r') => self.pending.len() - 1,
            _ => self.pending.len(),
        };
        if self.pending.len() > LINE_MAX {
            return Err(SseError::LineTooLong);
        }
        Ok(events)
    }

    /// One complete line: a field, a comment, or the blank line that dispatches.
    fn absorb(
        event: &mut Option<EventName>,
        data: &mut Vec<String>,
        data_len: &mut usize,
        line: &str,
    ) -> Result<Option<SseEvent>, SseError> {
        if line.len() > LINE_MAX {
            return Err(SseError::LineTooLong);
        }
        if line.is_empty() {
            let name = event.take();
            let lines = std::mem::take(data);
            *data_len = 0;
            return Ok((!lines.is_empty()).then(|| SseEvent {
                event: name,
                data: lines.join("\n"),
            }));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => *event = (!value.is_empty()).then(|| EventName(value.to_owned())),
            "data" => {
                *data_len += value.len() + 1;
                if *data_len > EVENT_DATA_MAX {
                    return Err(SseError::LineTooLong);
                }
                data.push(value.to_owned());
            }
            _ => {}
        }
        Ok(None)
    }
}

/// The end of the line that starts at `start` in `bytes` and where the next one starts, or `None`
/// when no line is complete yet. Bytes before `scan` are known to hold no line end.
fn line_end(bytes: &[u8], start: usize, scan: usize, stream: Stream) -> Option<(usize, usize)> {
    let at = scan
        + bytes
            .get(scan..)?
            .iter()
            .position(|b| matches!(b, b'\n' | b'\r'))?;
    debug_assert!(at >= start);
    match (bytes[at], bytes.get(at + 1)) {
        (b'\n', _) => Some((at, at + 1)),
        (_, Some(b'\n')) => Some((at, at + 2)),
        (_, Some(_)) => Some((at, at + 1)),
        (_, None) if stream == Stream::Over => Some((at, at + 1)),
        (_, None) => None,
    }
}
