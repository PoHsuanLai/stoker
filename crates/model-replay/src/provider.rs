//! The replaying and recording providers.

use model_provider::{ModelInfo, Provider, ProviderError, TurnEnd, TurnRequest, TurnSink};
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

/// Plays a cassette as a provider.
#[derive(Debug)]
pub struct ReplayProvider {
    cassette: Cassette,
    mode: ReplayMode,
}

impl ReplayProvider {
    pub fn new(cassette: Cassette, mode: ReplayMode) -> Self {
        Self { cassette, mode }
    }
}

impl Provider for ReplayProvider {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        let _ = &self.cassette.header;
        async { todo!("ReplayProvider::describe: the cassette's model") }
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let _ = (&self.mode, request, &mut *sink);
        async { todo!("ReplayProvider::turn: match, push events, return the end") }
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
}

impl<P: Provider, C: CassetteSink> RecordingProvider<P, C> {
    pub fn new(inner: P, sink: C) -> Self {
        Self { inner, sink }
    }
}

impl<P: Provider, C: CassetteSink> Provider for RecordingProvider<P, C> {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        self.inner.describe()
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let _ = (&self.inner, &self.sink, request, &mut *sink);
        async { todo!("RecordingProvider::turn: tee events into an Interaction, write it") }
    }
}
