//! The replaying and recording providers.

use std::sync::Mutex;

use model_provider::{
    Flow, ModelInfo, ModelName, Provider, ProviderError, StopReason, Tokens, TurnEnd, TurnEvent,
    TurnRequest, TurnSink, TurnUsage,
};
use serde::{Deserialize, Serialize};

use crate::{Cassette, Interaction, InteractionId, RequestPrint};

/// How a replay matches requests to interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// The nth turn gets the nth interaction.
    InOrder,
    /// Each turn gets the first unused interaction whose request print equals its own.
    ByRequest,
    /// The nth turn gets the nth interaction, and its request hash must equal too.
    Strict,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    #[error("the cassette has no interaction left")]
    Exhausted,
    #[error("interaction {index:?} was recorded for a different request")]
    Mismatch {
        index: InteractionId,
        want: Box<RequestPrint>,
        got: Box<RequestPrint>,
    },
}

/// Which interactions a replay has handed out, and the turns it could not serve.
#[derive(Debug, Default)]
struct Cursor {
    next: usize,
    used: Vec<usize>,
    misses: Vec<ReplayError>,
}

/// Plays a cassette as a provider.
#[derive(Debug)]
pub struct ReplayProvider {
    cassette: Cassette,
    mode: ReplayMode,
    cursor: Mutex<Cursor>,
}

impl ReplayProvider {
    pub fn new(cassette: Cassette, mode: ReplayMode) -> Self {
        Self {
            cassette,
            mode,
            cursor: Mutex::new(Cursor::default()),
        }
    }

    /// The turns that found no interaction, with the prints a mismatch compared (a provider
    /// can only answer `ProviderError`, so a test reads the detail here).
    pub fn misses(&self) -> Vec<ReplayError> {
        self.cursor
            .lock()
            .map(|c| c.misses.clone())
            .unwrap_or_default()
    }

    fn pick(&self, got: &RequestPrint) -> Result<Interaction, ReplayError> {
        let mut cursor = self.cursor.lock().map_err(|_| ReplayError::Exhausted)?;
        let picked = pick(&self.cassette.interactions, self.mode, &mut cursor, got);
        if let Err(miss) = &picked {
            cursor.misses.push(miss.clone());
        }
        picked.cloned()
    }
}

fn pick<'a>(
    interactions: &'a [Interaction],
    mode: ReplayMode,
    cursor: &mut Cursor,
    got: &RequestPrint,
) -> Result<&'a Interaction, ReplayError> {
    let mismatch = |it: &Interaction| ReplayError::Mismatch {
        index: it.id,
        want: Box::new(it.request.clone()),
        got: Box::new(got.clone()),
    };
    match mode {
        ReplayMode::InOrder | ReplayMode::Strict => {
            let it = interactions
                .get(cursor.next)
                .ok_or(ReplayError::Exhausted)?;
            cursor.next += 1;
            match mode {
                ReplayMode::Strict if it.print != got.hash() => Err(mismatch(it)),
                _ => Ok(it),
            }
        }
        ReplayMode::ByRequest => {
            let hash = got.hash();
            let mut unused = interactions
                .iter()
                .enumerate()
                .filter(|(at, _)| !cursor.used.contains(at));
            let first = unused.next().ok_or(ReplayError::Exhausted)?;
            let (at, it) = std::iter::once(first)
                .chain(unused)
                .find(|(_, it)| it.print == hash)
                .ok_or_else(|| mismatch(first.1))?;
            cursor.used.push(at);
            Ok(it)
        }
    }
}

impl ReplayError {
    fn into_provider(self) -> ProviderError {
        match self {
            ReplayError::Exhausted => ProviderError::NotReady,
            miss @ ReplayError::Mismatch { .. } => ProviderError::BadRequest(miss.to_string()),
        }
    }
}

/// Pushes the events, stopping when the sink says so (the turn then ends with `EndTurn`).
fn play<K: TurnSink>(
    interaction: Interaction,
    sink: &mut K,
    served: ModelName,
) -> Result<TurnEnd, ProviderError> {
    for event in interaction.events {
        if sink.event(event) == Flow::Stop {
            return Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: TurnUsage::default(),
                served,
            });
        }
    }
    interaction.end
}

impl Provider for ReplayProvider {
    /// The cassette's model. The header records no context size, so both are the largest
    /// value: a replayed conversation is never packed down.
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(vec![ModelInfo {
            name: self.cassette.header.model.clone(),
            loaded_context: Tokens(u32::MAX),
            trained_context: Tokens(u32::MAX),
        }]))
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let picked = self.pick(&RequestPrint::of(request));
        let served = self.cassette.header.model.clone();
        std::future::ready(
            picked
                .map_err(ReplayError::into_provider)
                .and_then(|it| play(it, sink, served)),
        )
    }
}

/// A sink for recorded interactions, injected so the crate itself writes no files.
pub trait CassetteSink: Send + Sync {
    fn write(&self, line: &Interaction) -> Result<(), SinkError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the cassette sink refused the write")]
pub struct SinkError;

/// Wraps a provider and records every turn it makes into a sink.
#[derive(Debug)]
pub struct RecordingProvider<P: Provider, C: CassetteSink> {
    inner: P,
    sink: C,
    next: Mutex<u32>,
}

impl<P: Provider, C: CassetteSink> RecordingProvider<P, C> {
    pub fn new(inner: P, sink: C) -> Self {
        Self {
            inner,
            sink,
            next: Mutex::new(0),
        }
    }

    fn take_id(&self) -> InteractionId {
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        let id = InteractionId(*next);
        *next += 1;
        id
    }
}

/// Passes events through and keeps a copy.
struct Tee<'a, K: TurnSink> {
    inner: &'a mut K,
    seen: Vec<TurnEvent>,
}

impl<K: TurnSink> TurnSink for Tee<'_, K> {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.seen.push(event.clone());
        self.inner.event(event)
    }
}

impl<P: Provider, C: CassetteSink> Provider for RecordingProvider<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    /// Runs the inner turn, then writes its interaction. A sink that refuses the write fails
    /// the turn (`Unreadable`): a recording that silently lost a turn would replay wrong.
    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let print = RequestPrint::of(request);
        async move {
            let mut tee = Tee {
                inner: sink,
                seen: Vec::new(),
            };
            let end = self.inner.turn(request, &mut tee).await;
            let interaction = Interaction {
                id: self.take_id(),
                print: print.hash(),
                request: print,
                events: tee.seen,
                end: end.clone(),
            };
            self.sink.write(&interaction).map_err(|_| {
                ProviderError::Unreadable("the cassette sink refused the write".into())
            })?;
            end
        }
    }
}
