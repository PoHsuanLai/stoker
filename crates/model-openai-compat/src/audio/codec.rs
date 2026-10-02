//! Speech request encoding and response decoding, with no I/O.

use serde::{Deserialize, Serialize};
use speech_provider::{AudioChunk, AudioFormat, HeardText, SttRequest, TtsRequest};

use crate::RequestJson;

/// Which server answers: they differ in the PCM they stream and where the voices are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechFlavor {
    /// vLLM's transcription endpoint (batch, WAV in, JSON out).
    Vllm,
    /// Kokoro-FastAPI: `/v1/audio/speech` with `response_format = "pcm"`, 24 kHz S16 mono, and
    /// `/v1/audio/voices`.
    KokoroFastApi,
}

/// Why a speech request cannot be encoded or a response cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AudioCodecError {
    #[error("this server cannot make or take the requested audio format")]
    UnsupportedFormat,
    #[error("the audio chunks do not share one format")]
    MixedFormats,
    #[error("the response is not the JSON the endpoint documents")]
    Unreadable,
}

/// A `multipart/form-data` body with the audio in it. `Debug` shows the length only.
#[derive(Clone, PartialEq, Eq)]
pub struct MultipartBody {
    /// The `Content-Type` header value, boundary included.
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl core::fmt::Debug for MultipartBody {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "MultipartBody(<{} bytes>)", self.bytes.len())
    }
}

/// The JSON body of `POST /audio/speech` for `request`: model, input, voice, `pcm`, stream on.
/// The format the request asks for must be the one `flavor` streams.
pub fn encode_speech_request(
    request: &TtsRequest,
    flavor: SpeechFlavor,
) -> Result<RequestJson, AudioCodecError> {
    let _ = (request, flavor);
    todo!("encode_speech_request: model, input, voice, response_format pcm, stream true")
}

/// The multipart body of `POST /audio/transcriptions`: the chunks as one WAV file, the model and
/// the language (`Prefer`'s first tag; `Auto` leaves it out), `response_format = json`.
pub fn encode_transcription(
    request: &SttRequest,
    audio: &[AudioChunk],
) -> Result<MultipartBody, AudioCodecError> {
    let _ = (request, audio);
    todo!("encode_transcription: WAV header over the samples, form fields, boundary")
}

/// The text of a transcription response.
pub fn decode_transcription(body: &[u8]) -> Result<HeardText, AudioCodecError> {
    let _ = body;
    todo!("decode_transcription: the `text` field of the JSON")
}

/// Cuts a streamed PCM body into chunks at sample boundaries, carrying an odd byte to the next
/// read, and numbers them from the start of the utterance.
#[derive(Debug, Clone)]
pub struct PcmDecoder {
    format: AudioFormat,
    carry: Vec<u8>,
    samples: u64,
}

impl PcmDecoder {
    pub fn new(format: AudioFormat) -> Self {
        Self {
            format,
            carry: Vec::new(),
            samples: 0,
        }
    }

    /// The whole samples in `bytes` (with the carry); at most one chunk, none for a short read.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<AudioChunk> {
        let _ = (&self.format, &mut self.carry, &mut self.samples, bytes);
        todo!("PcmDecoder::feed: join the carry, keep the partial sample, number the chunk")
    }
}
