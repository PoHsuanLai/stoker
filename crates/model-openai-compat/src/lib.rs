//! The chat-completions wire that llama-server, vLLM, LiteLLM and OpenRouter share.
//!
//! `OpenAiCodec` is a `model-wire` codec and pure: a `TurnRequest` becomes an `Exchange`, and the
//! server's SSE frames become `TurnEvent`s. the `Server` trait holds what differs between
//! servers. `OpenAiCompat` is `Driver<OpenAiCodec, HttpClient>`. The `audio` module is the speech
//! wire, still over `model-http`'s client directly.

mod assemble;
mod audio;
mod charge;
mod classify;
mod codec;
mod decode;
mod embed;
mod envelope;
mod leak;
mod logprobs;
mod messages;
mod models;
mod provider;
mod request;
mod server;

pub use audio::{
    AudioCodecError, KOKORO_FORMAT, KokoroSpeech, MultipartBody, OpenAiTranscription, PcmDecoder,
    decode_transcription, encode_speech_request, encode_transcription,
};
pub use charge::read_charge;
pub use codec::{Flavor, OpenAiCodec, RequestJson};
pub use decode::StreamDecoder;
pub use leak::LeakMarker;
pub use model_wire::CodecError;
pub use provider::OpenAiCompat;
pub use request::{encode_request, enforceable};
