//! The sink the driver hands a transport for a chat turn: the head decides what the body is
//! (a reply to decode, or an error to classify), and the framer the exchange named cuts the reply
//! into frames for the decoder.

use model_http::{
    BodySink, ChunkFlow, Framing, HttpError, HttpStatus, NdjsonDecoder, ResponseHead, SseDecoder,
};
use model_provider::{
    Flow, ModelName, ProviderError, StopReason, TurnEnd, TurnEvent, TurnSink, TurnUsage,
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

/// The `BodySink` of one chat turn.
pub(crate) struct ChatSink<'a, C: ChatCodec, K: TurnSink> {
    codec: &'a C,
    sink: &'a mut K,
    mode: Mode<C::Decoder>,
    framing: Framing,
    served: ModelName,
    usage: TurnUsage,
    head: Option<ResponseHead>,
}

impl<'a, C: ChatCodec, K: TurnSink> ChatSink<'a, C, K> {
    pub(crate) fn new(codec: &'a C, sink: &'a mut K, framing: Framing, served: ModelName) -> Self {
        Self {
            mode: Mode::Head(codec.decoder(served.clone())),
            codec,
            sink,
            framing,
            served,
            usage: TurnUsage::default(),
            head: None,
        }
    }

    /// Hands every event of `events` to the caller's sink; `Stop` ends the turn.
    fn deliver(&mut self, events: Vec<TurnEvent>) -> Flow {
        for event in events {
            if let TurnEvent::Usage(usage) = &event {
                self.usage = *usage;
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
        let step = framer
            .feed(bytes)
            .and_then(|frames| self.decode(&mut decoder, frames));
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

    /// The turn's result, once the transport is done with the exchange.
    pub(crate) fn conclude(
        mut self,
        result: Result<HttpStatus, HttpError>,
    ) -> Result<TurnEnd, ProviderError> {
        match std::mem::replace(&mut self.mode, Mode::Done) {
            Mode::Failed(error) => Err(error),
            Mode::Stopped => Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: self.usage,
                served: self.served,
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
        decoder
            .finish()
            .map_err(|e| fault.unwrap_or_else(|| codec_error(e)))
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
}
