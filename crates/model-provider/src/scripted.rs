//! A provider that plays scripted turns and records every request it was given.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::{
    Flow, ModelInfo, Provider, ProviderError, StopReason, TurnEnd, TurnEvent, TurnRequest,
    TurnSink, TurnUsage,
};

/// One scripted turn: the events to push, then how it ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub events: Vec<TurnEvent>,
    pub end: Result<TurnEnd, ProviderError>,
}

/// Plays one [`Script`] per turn, in order, and keeps the requests so a test can assert on
/// prompt assembly. A turn with no script left answers `NotReady`.
#[derive(Debug)]
pub struct ScriptedProvider {
    models: Vec<ModelInfo>,
    scripts: Mutex<VecDeque<Script>>,
    seen: Mutex<Vec<TurnRequest>>,
}

impl ScriptedProvider {
    pub fn new(models: Vec<ModelInfo>, scripts: Vec<Script>) -> Self {
        Self {
            models,
            scripts: Mutex::new(scripts.into()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every request the provider has been given, in order.
    pub fn requests(&self) -> Vec<TurnRequest> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl Provider for ScriptedProvider {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        std::future::ready(Ok(self.models.clone()))
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(request.clone());
        }
        let script = self
            .scripts
            .lock()
            .ok()
            .and_then(|mut scripts| scripts.pop_front());
        let served = request.model.clone();
        let result = match script {
            None => Err(ProviderError::NotReady),
            Some(script) => play(script, sink, served),
        };
        std::future::ready(result)
    }
}

fn play<K: TurnSink>(
    script: Script,
    sink: &mut K,
    served: crate::ModelName,
) -> Result<TurnEnd, ProviderError> {
    for event in script.events {
        if sink.event(event) == Flow::Stop {
            return Ok(TurnEnd {
                stop: StopReason::EndTurn,
                usage: TurnUsage::default(),
                served,
                first_token: None,
            });
        }
    }
    script.end
}
