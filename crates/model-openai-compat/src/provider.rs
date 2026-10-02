//! The provider over `model-http`.

use model_http::HttpClient;
use model_provider::{ModelInfo, Provider, ProviderError, TurnEnd, TurnRequest, TurnSink};

use crate::Flavor;

/// One OpenAI-compatible endpoint.
#[derive(Debug, Clone)]
pub struct OpenAiCompat {
    client: HttpClient,
    flavor: Flavor,
}

impl OpenAiCompat {
    pub fn new(client: HttpClient, flavor: Flavor) -> Self {
        Self { client, flavor }
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }
}

impl Provider for OpenAiCompat {
    fn describe(&self) -> impl Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send {
        let _ = &self.client;
        async { todo!("OpenAiCompat::describe: GET /models") }
    }

    fn turn<K: TurnSink>(
        &self,
        request: &TurnRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TurnEnd, ProviderError>> + Send {
        let _ = (&self.client, request, &mut *sink);
        async { todo!("OpenAiCompat::turn: encode, post, decode the stream into the sink") }
    }
}
