//! The speech endpoints of OpenAI-compatible servers: `POST /v1/audio/speech` streams PCM,
//! `POST /v1/audio/transcriptions` takes a WAV file and answers JSON. `codec` is pure;
//! `OpenAiSpeech` joins it to `model-http`.

mod codec;
mod provider;

pub use codec::{
    AudioCodecError, MultipartBody, PcmDecoder, SpeechFlavor, decode_transcription,
    encode_speech_request, encode_transcription,
};
pub use provider::OpenAiSpeech;
