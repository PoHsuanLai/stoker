//! Speech request encoding and response decoding, with no I/O.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use speech_provider::{
    AudioChunk, AudioFormat, HeardText, LangChoice, PcmFormat, SampleIndex, SampleRate, SttRequest,
    TtsRequest,
};

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

/// What Kokoro-FastAPI streams for `response_format = "pcm"`: 24 kHz signed 16-bit mono.
pub const KOKORO_FORMAT: AudioFormat = AudioFormat {
    rate: SampleRate(24_000),
    pcm: PcmFormat::S16Le,
};

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
    // Only Kokoro-FastAPI has the speech endpoint, and it streams one format.
    if flavor != SpeechFlavor::KokoroFastApi || request.format != KOKORO_FORMAT {
        return Err(AudioCodecError::UnsupportedFormat);
    }
    let body = json!({
        "model": request.model.0,
        "input": request.text.as_str(),
        "voice": request.voice.as_str(),
        "response_format": "pcm",
        "stream": true,
    });
    Ok(RequestJson(body.to_string()))
}

/// The multipart body of `POST /audio/transcriptions`: the chunks as one WAV file, the model and
/// the language (`Prefer`'s first tag; `Auto` leaves it out), `response_format = json`.
pub fn encode_transcription(
    request: &SttRequest,
    audio: &[AudioChunk],
) -> Result<MultipartBody, AudioCodecError> {
    let format = match audio.split_first() {
        Some((first, rest)) if rest.iter().any(|c| c.format != first.format) => {
            return Err(AudioCodecError::MixedFormats);
        }
        Some((first, _)) => first.format,
        None => request.format,
    };
    let pcm: Vec<u8> = audio
        .iter()
        .flat_map(|chunk| whole_samples(chunk, format))
        .copied()
        .collect();
    let wav = wav_file(format, &pcm)?;
    let mut boundary = String::from("----stoker0f3c9a1e");
    while contains(&wav, boundary.as_bytes()) {
        boundary.push('x');
    }
    let mut bytes = Vec::new();
    let mut field = |name: &str, value: &str| {
        bytes.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    };
    field("model", &request.model.0);
    field("response_format", "json");
    if let LangChoice::Prefer(langs) = &request.lang
        && let Some(first) = langs.first()
    {
        // The servers want the ISO 639-1 code: `zh`, not `zh-CN`.
        field("language", first.as_str().split('-').next().unwrap_or(""));
    }
    bytes.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    bytes.extend_from_slice(&wav);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok(MultipartBody {
        content_type: format!("multipart/form-data; boundary={boundary}"),
        bytes,
    })
}

/// The text of a transcription response.
pub fn decode_transcription(body: &[u8]) -> Result<HeardText, AudioCodecError> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(map)) => match map.get("text") {
            Some(Value::String(text)) => Ok(HeardText(text.clone())),
            _ => Err(AudioCodecError::Unreadable),
        },
        _ => Err(AudioCodecError::Unreadable),
    }
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
        self.carry.extend_from_slice(bytes);
        let width = usize::try_from(self.format.pcm.width()).unwrap_or(usize::MAX);
        let whole = self.carry.len() - self.carry.len() % width.max(1);
        if whole == 0 {
            return None;
        }
        let rest = self.carry.split_off(whole);
        let pcm = std::mem::replace(&mut self.carry, rest);
        let at = SampleIndex(self.samples);
        self.samples += (whole / width) as u64;
        Some(AudioChunk {
            format: self.format,
            at,
            pcm: speech_provider::PcmBytes::new(pcm),
        })
    }
}

/// The whole samples of a chunk: a trailing partial sample is not part of the audio.
fn whole_samples(chunk: &AudioChunk, format: AudioFormat) -> &[u8] {
    let width = usize::try_from(format.pcm.width())
        .unwrap_or(usize::MAX)
        .max(1);
    let bytes = chunk.pcm.as_slice();
    &bytes[..bytes.len() - bytes.len() % width]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A mono WAV file around `pcm`: the 44-byte header, then the samples. Signed 16-bit is format
/// 1, 32-bit float is format 3.
fn wav_file(format: AudioFormat, pcm: &[u8]) -> Result<Vec<u8>, AudioCodecError> {
    let (tag, bits) = match format.pcm {
        PcmFormat::S16Le => (1_u16, 16_u16),
        PcmFormat::F32Le => (3_u16, 32_u16),
    };
    let width = u32::from(bits / 8);
    let data = u32::try_from(pcm.len()).map_err(|_| AudioCodecError::UnsupportedFormat)?;
    let riff = data
        .checked_add(36)
        .ok_or(AudioCodecError::UnsupportedFormat)?;
    let byte_rate = format
        .rate
        .0
        .checked_mul(width)
        .ok_or(AudioCodecError::UnsupportedFormat)?;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&tag.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&format.rate.0.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&(bits / 8).to_le_bytes());
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.extend_from_slice(pcm);
    Ok(wav)
}
