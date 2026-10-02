//! The provider: the codec joined to `model-http`'s client by `model-wire`'s `Driver`.

use model_http::HttpClient;
use model_wire::Driver;

use crate::OpenAiCodec;

/// One OpenAI-compatible endpoint: build it with `OpenAiCodec::new(flavor).provider(client)`.
pub type OpenAiCompat = Driver<OpenAiCodec, HttpClient>;
