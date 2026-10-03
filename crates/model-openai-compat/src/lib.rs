//! The chat-completions wire that llama-server, vLLM, LiteLLM and OpenRouter share.
//!
//! `OpenAiCodec` is a `model-wire` codec and pure: a `TurnRequest` becomes an `Exchange`, and the
//! server's SSE frames become `TurnEvent`s. `Flavor::quirks` is the table of what differs between
//! servers. `OpenAiCompat` is `Driver<OpenAiCodec, HttpClient>`. The `audio` module is the speech
//! wire, still over `model-http`'s client directly.

mod assemble;
mod audio;
mod codec;
mod decode;
mod envelope;
mod provider;
mod quirks;

pub use audio::{
    AudioCodecError, MultipartBody, OpenAiSpeech, PcmDecoder, SpeechFlavor, decode_transcription,
    encode_speech_request, encode_transcription,
};
pub use codec::{Flavor, OpenAiCodec, RequestJson, encode_request};
pub use decode::StreamDecoder;
pub use model_wire::CodecError;
pub use provider::OpenAiCompat;
pub use quirks::{DimensionsField, Quirks, ToolImages, ToolNaming, UsageAsk};
