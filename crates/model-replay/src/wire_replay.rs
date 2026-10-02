//! Plays a wire cassette as a `Transport`.

use std::sync::Mutex;

use model_http::{BodySink, ChunkFlow, Exchange, HttpError, HttpStatus, ResponseHead, Transport};

use crate::wire_form::{chunks, is_success, request_of, same_request};
use crate::{
    ChunkPlan, InteractionId, ReplayMode, WireCassette, WireEnd, WireExchange, WireMiss, WireReply,
    WireRequest,
};

#[derive(Debug, Default)]
struct Cursor {
    next: usize,
    used: Vec<usize>,
    misses: Vec<WireMiss>,
}

/// Plays a wire cassette as a transport: the recorded head, then the recorded body sliced by the
/// plan.
///
/// A request with no exchange to serve it answers `HttpError::Connect` and is kept in
/// [`ReplayTransport::misses`]. A recorded `Reset` ends in `HttpError::Broken` after the
/// frames; a non-success head ends in `Rejected`.
#[derive(Debug)]
pub struct ReplayTransport {
    cassette: WireCassette,
    mode: ReplayMode,
    plan: ChunkPlan,
    cursor: Mutex<Cursor>,
}

impl ReplayTransport {
    pub fn new(cassette: WireCassette, mode: ReplayMode, plan: ChunkPlan) -> Self {
        Self {
            cassette,
            mode,
            plan,
            cursor: Mutex::new(Cursor::default()),
        }
    }

    /// The requests that found no exchange, with what a mismatch compared.
    pub fn misses(&self) -> Vec<WireMiss> {
        self.cursor
            .lock()
            .map(|c| c.misses.clone())
            .unwrap_or_default()
    }

    fn pick(&self, got: &WireRequest) -> Result<WireReply, WireMiss> {
        let mut cursor = self.cursor.lock().unwrap_or_else(|e| e.into_inner());
        let picked = pick(&self.cassette.exchanges, self.mode, &mut cursor, got);
        if let Err(miss) = &picked {
            cursor.misses.push(miss.clone());
        }
        picked.map(|ex| ex.reply.clone())
    }
}

fn pick<'a>(
    exchanges: &'a [WireExchange],
    mode: ReplayMode,
    cursor: &mut Cursor,
    got: &WireRequest,
) -> Result<&'a WireExchange, WireMiss> {
    let mismatch = |at: usize, ex: &WireExchange| WireMiss::Mismatch {
        index: InteractionId(u32::try_from(at).unwrap_or(u32::MAX)),
        want: Box::new(ex.request.clone()),
        got: Box::new(got.clone()),
    };
    match mode {
        ReplayMode::InOrder | ReplayMode::Strict => {
            let at = cursor.next;
            let ex = exchanges.get(at).ok_or(WireMiss::Exhausted)?;
            cursor.next += 1;
            match mode {
                ReplayMode::Strict if !same_request(&ex.request, got) => Err(mismatch(at, ex)),
                _ => Ok(ex),
            }
        }
        ReplayMode::ByRequest => {
            let mut unused = exchanges
                .iter()
                .enumerate()
                .filter(|(at, _)| !cursor.used.contains(at));
            let first = unused.next().ok_or(WireMiss::Exhausted)?;
            let (at, ex) = std::iter::once(first)
                .chain(unused)
                .find(|(_, ex)| same_request(&ex.request, got))
                .ok_or_else(|| mismatch(first.0, first.1))?;
            cursor.used.push(at);
            Ok(ex)
        }
    }
}

/// How a delivered response ends: the status for a success, `Rejected` for anything else, and
/// `Broken` for a connection that was cut after frames.
fn finish(status: HttpStatus, end: WireEnd) -> Result<HttpStatus, HttpError> {
    match (end, is_success(status)) {
        (WireEnd::Reset, _) => Err(HttpError::Broken),
        (_, true) => Ok(status),
        (_, false) => Err(HttpError::Rejected),
    }
}

fn deliver<K: BodySink>(
    reply: WireReply,
    ex: &Exchange,
    plan: ChunkPlan,
    sink: &mut K,
) -> Result<HttpStatus, HttpError> {
    let head = ResponseHead {
        status: reply.head.status,
        body: reply.head.body,
        retry_after: reply.head.retry_after,
        request_id: None,
    };
    if sink.head(&head) == ChunkFlow::Stop {
        return finish(head.status, WireEnd::Complete);
    }
    for chunk in chunks(&reply.body, ex.framing, plan) {
        if sink.chunk(&chunk) == ChunkFlow::Stop {
            return finish(head.status, WireEnd::Complete);
        }
    }
    finish(head.status, reply.end)
}

impl Transport for ReplayTransport {
    fn exchange<K: BodySink>(
        &self,
        ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let result = match self.pick(&request_of(ex)) {
            Ok(reply) => deliver(reply, ex, self.plan, sink),
            Err(_) => Err(HttpError::Connect),
        };
        std::future::ready(result)
    }
}
