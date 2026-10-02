//! Records the exchanges a `Transport` carries.

use model_http::{
    BodyKind, BodySink, ChunkFlow, Exchange, Framing, HttpError, HttpStatus, NdjsonDecoder,
    ResponseHead, SseDecoder, Transport,
};

use crate::wire_form::{head_print, request_of, scrub_text};
use crate::{WireBody, WireEnd, WireExchange, WireFrame, WireReply, WireSink};

/// Wraps any transport and records every exchange it carries into a sink.
///
/// The caller's sink sees everything as it arrives. Once the exchange ends, the head and the
/// bytes become a [`WireExchange`]: the body as frames when the head says event stream or NDJSON,
/// whole otherwise; scrubbed (model paths, image data). An exchange that never got a head
/// (connect failure, timeout) is not recorded. A wire sink that refuses the write fails the
/// exchange with `HttpError::Broken`: a recording that lost an exchange would replay wrong.
#[derive(Debug)]
pub struct RecordingTransport<T: Transport, S: WireSink> {
    inner: T,
    sink: S,
}

impl<T: Transport, S: WireSink> RecordingTransport<T, S> {
    pub fn new(inner: T, sink: S) -> Self {
        Self { inner, sink }
    }
}

/// Whether the caller's sink asked to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Consumer {
    Reading,
    Stopped,
}

/// Passes head and chunks on and keeps a copy.
struct Tee<'a, K: BodySink> {
    inner: &'a mut K,
    head: Option<ResponseHead>,
    bytes: Vec<u8>,
    consumer: Consumer,
}

impl<K: BodySink> Tee<'_, K> {
    fn pass(&mut self, flow: ChunkFlow) -> ChunkFlow {
        if flow == ChunkFlow::Stop {
            self.consumer = Consumer::Stopped;
        }
        flow
    }
}

impl<K: BodySink> BodySink for Tee<'_, K> {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.head = Some(head.clone());
        let flow = self.inner.head(head);
        self.pass(flow)
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.bytes.extend_from_slice(bytes);
        let flow = self.inner.chunk(bytes);
        self.pass(flow)
    }
}

/// The body as frames when the head and the framing agree on a framed stream.
fn body_of(kind: BodyKind, framing: Framing, bytes: &[u8]) -> WireBody {
    let whole = || WireBody::Whole(scrub_text(&String::from_utf8_lossy(bytes)));
    match (kind, framing) {
        (BodyKind::EventStream, Framing::Sse) => {
            let mut decoder = SseDecoder::new();
            match decoder.feed(bytes) {
                Ok(mut events) => {
                    events.extend(decoder.finish());
                    WireBody::Frames(
                        events
                            .into_iter()
                            .map(|e| WireFrame {
                                event: e.event,
                                data: scrub_text(&e.data),
                            })
                            .collect(),
                    )
                }
                Err(_) => whole(),
            }
        }
        (BodyKind::NdJson, Framing::Ndjson) => {
            let mut decoder = NdjsonDecoder::new();
            match decoder.feed(bytes) {
                Ok(mut lines) => {
                    lines.extend(decoder.finish());
                    WireBody::Frames(
                        lines
                            .into_iter()
                            .map(|data| WireFrame {
                                event: None,
                                data: scrub_text(&data),
                            })
                            .collect(),
                    )
                }
                Err(_) => whole(),
            }
        }
        _ => whole(),
    }
}

/// `Reset` when the connection failed after the head, `Cut` when the caller stopped reading,
/// `Complete` otherwise (a `Rejected` answer is a complete reply with a failing head).
fn end_of(result: &Result<HttpStatus, HttpError>, consumer: Consumer) -> WireEnd {
    match (result, consumer) {
        (Ok(_) | Err(HttpError::Rejected), Consumer::Reading) => WireEnd::Complete,
        (Ok(_) | Err(HttpError::Rejected), Consumer::Stopped) => WireEnd::Cut,
        (Err(_), _) => WireEnd::Reset,
    }
}

impl<T: Transport, S: WireSink> Transport for RecordingTransport<T, S> {
    async fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> Result<HttpStatus, HttpError> {
        let mut tee = Tee {
            inner: sink,
            head: None,
            bytes: Vec::new(),
            consumer: Consumer::Reading,
        };
        let result = self.inner.exchange(ex, &mut tee).await;
        if let Some(head) = &tee.head {
            let recorded = WireExchange {
                request: request_of(ex),
                reply: WireReply {
                    head: head_print(head),
                    body: body_of(head.body, ex.framing, &tee.bytes),
                    end: end_of(&result, tee.consumer),
                },
            };
            self.sink.write(&recorded).map_err(|_| HttpError::Broken)?;
        }
        result
    }
}
