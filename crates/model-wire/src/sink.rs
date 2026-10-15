//! The sink the driver hands a transport for a chat turn: the head decides what the body is
//! (a reply to decode, or an error to classify), and the framer the exchange named cuts the reply
//! into frames for the decoder.

use model_http::{
    BodySink, ChunkFlow, Framing, HttpError, HttpStatus, NdjsonDecoder, Progress, ResponseHead,
    SseDecoder, WaitMs,
};
use model_provider::{
    Flow, ModelName, ProviderError, StopReason, TurnEnd, TurnEvent, TurnSink, TurnUsage,
    UsageReport,
};

use crate::collect::{Collect, REPLY_BODY_MAX, is_failure};
use crate::errors::{codec_error, http_error, with_retry_after};
use crate::{ChatCodec, ChatDecoder, CodecError};

/// Cuts bytes into the frames the codec reads.
enum Framer {
    Sse(SseDecoder),
    Ndjson(NdjsonDecoder),
    /// The whole body is one frame, read once the stream is over.
    Whole(Vec<u8>),
}

impl Framer {
    fn new(framing: Framing) -> Self {
        match framing {
            Framing::Sse => Framer::Sse(SseDecoder::new()),
            Framing::Ndjson => Framer::Ndjson(NdjsonDecoder::new()),
            // Whole, and any framing this build does not know: read the body as one frame.
            _ => Framer::Whole(Vec::new()),
        }
    }

    /// The frames `bytes` completed. A stream that cannot be framed is an unreadable reply.
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, ProviderError> {
        match self {
            Framer::Sse(decoder) => decoder
                .feed(bytes)
                .map(|events| events.into_iter().map(|e| e.data).collect())
                .map_err(|e| ProviderError::Unreadable(e.to_string())),
            Framer::Ndjson(decoder) => decoder
                .feed(bytes)
                .map_err(|e| ProviderError::Unreadable(e.to_string())),
            Framer::Whole(body) => {
                body.extend_from_slice(bytes);
                if body.len() > REPLY_BODY_MAX {
                    return Err(ProviderError::Unreadable("the reply is too large".into()));
                }
                Ok(Vec::new())
            }
        }
    }

    /// The frames the end of the stream completes: a final `\r`, an unterminated line, the whole
    /// body.
    fn finish(self) -> Result<Vec<String>, ProviderError> {
        match self {
            Framer::Sse(decoder) => Ok(decoder.finish().into_iter().map(|e| e.data).collect()),
            Framer::Ndjson(decoder) => Ok(decoder.finish().into_iter().collect()),
            Framer::Whole(body) => String::from_utf8(body)
                .map(|text| vec![text])
                .map_err(|_| ProviderError::Unreadable("the reply is not UTF-8".into())),
        }
    }
}

/// The most raw body bytes one streamed reply may carry, comments and empty events included. The
/// codec caps the frames it reads; this caps what the wire carries, so a stream of keep-alive
/// lines ends too.
const STREAM_BYTES_MAX: usize = 80 << 20;

/// Whether `frames` hold anything: a stream of comments or empty events makes no progress.
fn progress_of(framing: Framing, frames: &[String]) -> Progress {
    match framing {
        Framing::Sse | Framing::Ndjson if frames.iter().all(|f| f.trim().is_empty()) => {
            Progress::Idle
        }
        _ => Progress::Made,
    }
}

/// Where the reply has got to.
enum Mode<D> {
    /// Waiting for the head; the decoder is ready.
    Head(D),
    /// A reply to decode.
    Reading {
        decoder: D,
        framer: Framer,
    },
    /// An error reply (or an HTML page): the body is kept for the classifier.
    Failing(Collect),
    /// The sink asked to stop: the turn ends early with what was seen.
    Stopped,
    /// A frame could not be read; the rest of the stream is not.
    Failed(ProviderError),
    Done,
}

/// Whether the server has sent a usage count in this stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Counted {
    No,
    Yes,
}

/// Whether the stream ran to its end (the decoder finished it) or was cut short (stopped by the
/// sink, failed, or dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Whole,
    Cut,
}

/// Whether the request reached a model at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reached {
    /// Refused, or never sent: nothing ran.
    Nothing,
    Model,
}

/// What a turn that is dropped before `conclude` may have cost.
fn interrupted<D>(mode: &Mode<D>, seen: TurnUsage) -> UsageReport {
    match mode {
        Mode::Failing(_) => UsageReport::Nothing,
        _ => UsageReport::Unknown { seen },
    }
}

/// The `BodySink` of one chat turn. When it drops, however the turn ended, it tells the caller's
/// sink what the attempt may have cost ([`TurnSink::ended`]).
pub(crate) struct ChatSink<'a, C: ChatCodec, K: TurnSink> {
    codec: &'a C,
    sink: &'a mut K,
    mode: Mode<C::Decoder>,
    framing: Framing,
    served: ModelName,
    usage: TurnUsage,
    head: Option<ResponseHead>,
    deadline: Option<WaitMs>,
    progress: Progress,
    raw: usize,
    counted: Counted,
    ending: Ending,
    report: Option<UsageReport>,
}

impl<'a, C: ChatCodec, K: TurnSink> ChatSink<'a, C, K> {
    pub(crate) fn new(
        codec: &'a C,
        sink: &'a mut K,
        framing: Framing,
        served: ModelName,
        deadline: Option<WaitMs>,
    ) -> Self {
        Self {
            mode: Mode::Head(codec.decoder(served.clone())),
            codec,
            sink,
            framing,
            served,
            usage: TurnUsage::default(),
            head: None,
            deadline,
            progress: Progress::Made,
            raw: 0,
            counted: Counted::No,
            ending: Ending::Cut,
            report: None,
        }
    }

    /// Hands every event of `events` to the caller's sink; `Stop` ends the turn.
    fn deliver(&mut self, events: Vec<TurnEvent>) -> Flow {
        for event in events {
            if let TurnEvent::Usage(usage) = &event {
                self.usage = *usage;
                self.counted = Counted::Yes;
            }
            if self.sink.event(event) == Flow::Stop {
                return Flow::Stop;
            }
        }
        Flow::Continue
    }

    /// Feeds frames to the decoder: its events go to the sink, its failure ends the stream (a
    /// fault the wire carried, else a generic unreadable reply).
    fn decode(
        &mut self,
        decoder: &mut C::Decoder,
        frames: Vec<String>,
    ) -> Result<Flow, ProviderError> {
        for frame in frames {
            let events = decoder
                .feed(&frame)
                .map_err(|e| fault_of(decoder, e, self.head.as_ref()))?;
            if self.deliver(events) == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }

    fn read(&mut self, mut decoder: C::Decoder, mut framer: Framer, bytes: &[u8]) -> ChunkFlow {
        self.raw = self.raw.saturating_add(bytes.len());
        let step = if self.raw > STREAM_BYTES_MAX {
            Err(ProviderError::Unreadable("the reply is too large".into()))
        } else {
            framer.feed(bytes).and_then(|frames| {
                self.progress = progress_of(self.framing, &frames);
                self.decode(&mut decoder, frames)
            })
        };
        match step {
            Ok(Flow::Continue) => {
                self.mode = Mode::Reading { decoder, framer };
                ChunkFlow::Continue
            }
            Ok(Flow::Stop) => {
                self.mode = Mode::Stopped;
                ChunkFlow::Stop
            }
            Err(error) => {
                self.mode = Mode::Failed(error);
                ChunkFlow::Stop
            }
        }
    }

    /// The turn's result, once the transport is done with the exchange; the usage report for the
    /// caller's sink follows from how it ended.
    pub(crate) fn conclude(
        mut self,
        result: Result<HttpStatus, HttpError>,
    ) -> Result<TurnEnd, ProviderError> {
        let reached = match (&self.mode, &result) {
            (Mode::Failing(_), _) => Reached::Nothing,
            (
                Mode::Head(_) | Mode::Done,
                Err(
                    HttpError::Connect
                    | HttpError::Tls
                    | HttpError::Insecure
                    | HttpError::ReplayMiss,
                ),
            ) => Reached::Nothing,
            _ => Reached::Model,
        };
        let end = self.settle(result);
        self.report = Some(match (reached, &end) {
            (Reached::Nothing, _) => UsageReport::Nothing,
            (Reached::Model, Ok(end))
                if self.ending == Ending::Whole && self.counted == Counted::Yes =>
            {
                UsageReport::Known(end.usage)
            }
            (Reached::Model, _) => UsageReport::Unknown { seen: self.usage },
        });
        end
    }

    fn settle(&mut self, result: Result<HttpStatus, HttpError>) -> Result<TurnEnd, ProviderError> {
        match std::mem::replace(&mut self.mode, Mode::Done) {
            Mode::Failed(error) => Err(error),
            Mode::Stopped => Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: self.usage,
                served: self.served.clone(),
                first_token: None,
            }),
            Mode::Failing(collect) => collect.into_reply(self.codec, result).and_then(|_| {
                Err(ProviderError::Unreadable(
                    "an error reply that is not an error".into(),
                ))
            }),
            Mode::Head(_) | Mode::Done => Err(result.err().map_or_else(
                || ProviderError::Unreadable("no response".into()),
                http_error,
            )),
            Mode::Reading { decoder, framer } => {
                let end = self.finish_stream(decoder, framer);
                match (result, end) {
                    (Ok(_), end) => end,
                    // The connection failed after the turn was complete: the turn stands.
                    (Err(_), Ok(end)) => Ok(end),
                    (Err(error), Err(_)) => Err(http_error(error)),
                }
            }
        }
    }

    fn finish_stream(
        &mut self,
        mut decoder: C::Decoder,
        framer: Framer,
    ) -> Result<TurnEnd, ProviderError> {
        let frames = framer.finish()?;
        if self.decode(&mut decoder, frames)? == Flow::Stop {
            return Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: self.usage,
                served: self.served.clone(),
                first_token: None,
            });
        }
        let fault = decoder.fault();
        let end = decoder
            .finish()
            .map_err(|e| fault.unwrap_or_else(|| codec_error(e)))?;
        self.ending = Ending::Whole;
        Ok(end)
    }
}

impl<C: ChatCodec, K: TurnSink> Drop for ChatSink<'_, C, K> {
    fn drop(&mut self) {
        let report = self
            .report
            .take()
            .unwrap_or_else(|| interrupted(&self.mode, self.usage));
        self.sink.ended(report);
    }
}

/// The error a decoder failure means: the wire's own fault (an error envelope inside a 200) when
/// it has one, with the head's `Retry-After` for a rate limit; otherwise the codec error.
fn fault_of<D: ChatDecoder>(
    decoder: &D,
    error: CodecError,
    head: Option<&ResponseHead>,
) -> ProviderError {
    match decoder.fault() {
        Some(fault) => with_retry_after(fault, head),
        None => codec_error(error),
    }
}

impl<C: ChatCodec, K: TurnSink> BodySink for ChatSink<'_, C, K> {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.head = Some(head.clone());
        let mode = std::mem::replace(&mut self.mode, Mode::Done);
        self.mode = match mode {
            Mode::Head(_) if is_failure(head) => {
                let mut collect = Collect::default();
                collect.head(head);
                Mode::Failing(collect)
            }
            Mode::Head(decoder) => Mode::Reading {
                decoder,
                framer: Framer::new(self.framing),
            },
            other => other,
        };
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        match std::mem::replace(&mut self.mode, Mode::Done) {
            Mode::Reading { decoder, framer } => self.read(decoder, framer, bytes),
            Mode::Failing(mut collect) => {
                let flow = collect.chunk(bytes);
                self.mode = Mode::Failing(collect);
                flow
            }
            other => {
                self.mode = other;
                ChunkFlow::Stop
            }
        }
    }

    fn progress(&self) -> Progress {
        self.progress
    }

    fn deadline(&self) -> Option<WaitMs> {
        self.deadline
    }
}
