//! The speech endpoints of OpenAI-compatible servers: `POST /v1/audio/speech` streams PCM,
//! `POST /v1/audio/transcriptions` takes a WAV file and answers JSON. `codec` is pure;
//! `OpenAiTranscription` and `KokoroSpeech` join it to `model-http`.

mod codec;
mod provider;
mod reply;

pub use codec::{
    AudioCodecError, KOKORO_FORMAT, MultipartBody, PcmDecoder, decode_transcription,
    encode_speech_request, encode_transcription,
};
pub use provider::{KokoroSpeech, OpenAiTranscription};
