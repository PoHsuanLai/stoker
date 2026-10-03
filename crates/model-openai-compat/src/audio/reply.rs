//! What the speech provider reads back: a body kept whole (a transcription, a model list), a
//! streamed PCM body cut into chunks, and the two listings.

use model_http::{BodyKind, BodySink, ChunkFlow, HttpError, HttpStatus, ResponseHead};
use model_provider::{Flow, ModelName, ProviderError};
use model_wire::http_error;
use serde_json::Value;
use speech_provider::{AudioFormat, AudioMs, AudioSink, VoiceId};

use super::PcmDecoder;
use crate::classify::classify;

/// An error reply is read to this many bytes (the classifier needs the envelope, not a page).
const ERROR_BODY_MAX: usize = 64 << 10;
/// A reply read whole (a transcription, a list) is refused above this many bytes.
const REPLY_BODY_MAX: usize = 4 << 20;

/// A non-success status, or an HTML page served as 200: what `classify` reads.
fn is_failure(head: &ResponseHead) -> bool {
    !(200..300).contains(&head.status.0) || head.body == BodyKind::Html
}

/// Keeps the head and the body, up to a cap that depends on the head.
#[derive(Debug, Default)]
pub(super) struct Whole {
    head: Option<ResponseHead>,
    body: Vec<u8>,
    capped: bool,
}

impl Whole {
    fn failed(&self) -> bool {
        self.head.as_ref().is_some_and(is_failure)
    }

    fn keep(&mut self, bytes: &[u8]) -> ChunkFlow {
        let cap = if self.failed() {
            ERROR_BODY_MAX
        } else {
            REPLY_BODY_MAX
        };
        let room = cap.saturating_sub(self.body.len());
        self.body.extend_from_slice(&bytes[..bytes.len().min(room)]);
        self.capped |= bytes.len() > room;
        if self.capped {
            ChunkFlow::Stop
        } else {
            ChunkFlow::Continue
        }
    }

    fn set_head(&mut self, head: &ResponseHead) {
        self.head = Some(head.clone());
    }

    /// The body of a success; a refusal, an HTML page, a body over the cap or a transport error is
    /// the `ProviderError` it means. Nothing of the body travels into the error.
    pub(super) fn into_body(
        self,
        sent: Result<HttpStatus, HttpError>,
    ) -> Result<Vec<u8>, ProviderError> {
        self.verdict(sent).map(|()| self.body)
    }

    fn verdict(&self, sent: Result<HttpStatus, HttpError>) -> Result<(), ProviderError> {
        match (sent, &self.head) {
            (Err(HttpError::Rejected), Some(head)) => Err(classify(head, &self.body)),
            (Err(error), _) => Err(http_error(error)),
            (Ok(_), Some(head)) if is_failure(head) => Err(classify(head, &self.body)),
            (Ok(_), Some(_)) if self.capped => {
                Err(ProviderError::Unreadable("the reply is too large".into()))
            }
            (Ok(_), Some(_)) => Ok(()),
            (Ok(_), None) => Err(ProviderError::Unreadable("the reply has no head".into())),
        }
    }
}

impl BodySink for Whole {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.set_head(head);
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.keep(bytes)
    }
}

/// Cuts a streamed PCM body into chunks for an [`AudioSink`]. A refusal's body is kept (capped)
/// for the classifier instead of being played. `Flow::Stop` from the sink closes the connection.
pub(super) struct Pcm<'a, K> {
    decoder: PcmDecoder,
    rate: u32,
    out: &'a mut K,
    reply: Whole,
    samples: u64,
    stopped: bool,
}

impl<'a, K: AudioSink> Pcm<'a, K> {
    pub(super) fn new(format: AudioFormat, out: &'a mut K) -> Self {
        Self {
            decoder: PcmDecoder::new(format),
            rate: format.rate.0,
            out,
            reply: Whole::default(),
            samples: 0,
            stopped: false,
        }
    }

    /// How much audio went to the sink, or the error the reply was. A reply with no audio in it
    /// is an error unless the sink had already stopped it.
    pub(super) fn into_played(
        self,
        sent: Result<HttpStatus, HttpError>,
    ) -> Result<AudioMs, ProviderError> {
        self.reply.verdict(sent)?;
        match (self.samples, self.stopped) {
            (0, false) => Err(ProviderError::Unreadable("the reply held no audio".into())),
            (samples, _) => {
                let ms = samples
                    .saturating_mul(1000)
                    .checked_div(u64::from(self.rate))
                    .unwrap_or(0);
                Ok(AudioMs(u32::try_from(ms).unwrap_or(u32::MAX)))
            }
        }
    }
}

impl<K: AudioSink> BodySink for Pcm<'_, K> {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.reply.set_head(head);
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        if self.reply.failed() {
            return self.reply.keep(bytes);
        }
        let Some(audio) = self.decoder.feed(bytes) else {
            return ChunkFlow::Continue;
        };
        self.samples += u64::from(audio.samples());
        match self.out.chunk(audio) {
            Flow::Continue => ChunkFlow::Continue,
            Flow::Stop => {
                self.stopped = true;
                ChunkFlow::Stop
            }
        }
    }
}

/// The model ids of a `GET /models` body (`{"data": [{"id": ..}]}`).
pub(super) fn listed_models(body: &[u8]) -> Result<Vec<ModelName>, ProviderError> {
    let unreadable =
        || ProviderError::Unreadable("the model list is not the documented JSON".into());
    let value: Value = serde_json::from_slice(body).map_err(|_| unreadable())?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    entries
        .iter()
        .map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(|id| ModelName(id.to_owned()))
                .ok_or_else(unreadable)
        })
        .collect()
}

/// The voices of a `GET /audio/voices` body: `{"voices": [..]}` or a bare array of names. A name
/// that is not a voice id is left out.
pub(super) fn listed_voices(body: &[u8]) -> Result<Vec<VoiceId>, ProviderError> {
    let unreadable =
        || ProviderError::Unreadable("the voice list is not the documented JSON".into());
    let value: Value = serde_json::from_slice(body).map_err(|_| unreadable())?;
    let names = match &value {
        Value::Array(names) => names,
        other => other
            .get("voices")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?,
    };
    Ok(names
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|name| VoiceId::new(name).ok())
        .collect())
}
