//! Joins a codec to a transport.

use model_http::Transport;
use model_provider::{
    Count, Dims, EmbedEnd, EmbedFault, EmbedTurn, Embedder, Knob, ModelInfo, Provider,
    ProviderError, TurnEnd, TurnRequest, TurnSink, TurnUsage,
};

use crate::collect::Collect;
use crate::errors::codec_error;
use crate::sink::ChatSink;
use crate::{ChatCodec, EmbedCodec};

/// A `Provider` made of a codec and a transport. Generic, never `dyn`: `inferd` keeps a closed
/// enum of the instantiations it builds.
///
/// `turn` encodes, then exchanges through a sink adapter. On the head: a non-success status, or
/// an `Html` body served as 200, is buffered (64 KiB at most) and mapped with `classify`;
/// otherwise the framer the exchange named runs and each frame goes to the decoder, whose events
/// go to the caller's `TurnSink`. `Flow::Stop` answers `ChunkFlow::Stop`.
///
/// An error envelope inside a 200 stream is the decoder's `fault`; a rate limit that carries no
/// seconds takes the head's `Retry-After`. A transport that cannot connect, times out or breaks
/// is `Unreachable` or `Timeout` (no body, nothing the prompt could be in).
#[derive(Debug, Clone)]
pub struct Driver<C, T> {
    codec: C,
    transport: T,
}

impl<C, T> Driver<C, T> {
    pub fn new(codec: C, transport: T) -> Self {
        Self { codec, transport }
    }

    pub fn codec(&self) -> &C {
        &self.codec
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }
}

impl<C: ChatCodec, T: Transport> Provider for Driver<C, T> {
    async fn describe(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let exchange = self.codec.describe();
        let mut collect = Collect::default();
        let result = self.transport.exchange(&exchange, &mut collect).await;
        let (_, body) = collect.into_reply(&self.codec, result)?;
        self.codec.parse_models(&body).map_err(codec_error)
    }

    async fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> Result<TurnEnd, ProviderError> {
        let exchange = self.codec.encode(request).map_err(codec_error)?;
        let mut adapter = ChatSink::new(&self.codec, sink, exchange.framing, request.model.clone());
        let result = self.transport.exchange(&exchange, &mut adapter).await;
        adapter.conclude(result)
    }
}

impl<C: EmbedCodec, T: Transport> Embedder for Driver<C, T> {
    async fn embed(&self, turn: &EmbedTurn) -> Result<EmbedEnd, ProviderError> {
        if turn.inputs.is_empty() {
            return Ok(EmbedEnd {
                vectors: Vec::new(),
                usage: TurnUsage::default(),
                served: turn.model.clone(),
            });
        }
        let exchange = self.codec.encode_embed(turn).map_err(codec_error)?;
        let mut collect = Collect::default();
        let result = self.transport.exchange(&exchange, &mut collect).await;
        let (_, body) = collect.into_reply(&self.codec, result)?;
        let end = self
            .codec
            .decode_embed(turn.model.clone(), &body)
            .map_err(codec_error)?;
        check_reply(&end, turn)?;
        Ok(end)
    }
}

/// One vector per input, each as wide as the width asked for (or, when none was asked for, as
/// wide as the first): a wrong-width vector never reaches a store.
fn check_reply(end: &EmbedEnd, turn: &EmbedTurn) -> Result<(), ProviderError> {
    let want = Count(u32::try_from(turn.inputs.len()).unwrap_or(u32::MAX));
    let dims = match turn.dims {
        Knob::Set(dims) => dims,
        Knob::Off => Dims(
            end.vectors
                .first()
                .map_or(0, |v| u32::try_from(v.0.len()).unwrap_or(u32::MAX)),
        ),
    };
    end.check(want, dims).map_err(|fault| match fault {
        EmbedFault::CountMismatch { want, got } => {
            ProviderError::Unreadable(format!("asked for {} vectors, got {}", want.0, got.0))
        }
        EmbedFault::WidthMismatch { want, got } => ProviderError::Unreadable(format!(
            "expected vectors of width {}, got {}",
            want.0, got.0
        )),
    })
}
