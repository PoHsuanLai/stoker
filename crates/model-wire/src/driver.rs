//! Joins a codec to a transport.

use model_http::Transport;
use model_provider::{
    EmbedEnd, EmbedTurn, Embedder, ModelInfo, Provider, ProviderError, TurnEnd, TurnRequest,
    TurnSink,
};

use crate::{ChatCodec, EmbedCodec};

/// A `Provider` made of a codec and a transport. Generic, never `dyn`: `inferd` keeps a closed
/// enum of the instantiations it builds.
///
/// `turn` encodes, then exchanges through a sink adapter. On the head: a non-success status, or
/// an `Html` body served as 200, is buffered (64 KiB at most) and mapped with `classify`;
/// otherwise the framer the exchange named runs and each frame goes to the decoder, whose events
/// go to the caller's `TurnSink`. `Flow::Stop` answers `ChunkFlow::Stop`.
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
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        let _ = (&self.codec, &self.transport);
        async { todo!("Driver::describe: exchange codec.describe(), parse_models") }
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let _ = (&self.codec, &self.transport, request, &mut *sink);
        async {
            todo!(
                "Driver::turn: encode, exchange, classify on a bad head, framer into decoder into the sink"
            )
        }
    }
}

impl<C: EmbedCodec, T: Transport> Embedder for Driver<C, T> {
    fn embed(
        &self,
        turn: &EmbedTurn,
    ) -> impl Future<Output = Result<EmbedEnd, ProviderError>> + Send {
        let _ = (&self.codec, &self.transport, turn);
        async {
            todo!(
                "Driver::embed: encode_embed, exchange, decode_embed, the per-reply count and width check"
            )
        }
    }
}
