//! A sink that keeps a whole body: what `describe`, `embed` and an error reply need.

use model_http::{BodyKind, BodySink, ChunkFlow, HttpError, HttpStatus, ResponseHead};
use model_provider::ProviderError;

use crate::ErrorWire;
use crate::errors::{http_error, with_retry_after};

/// An error reply is read to this many bytes (the classifier needs the envelope, not a page).
pub(crate) const ERROR_BODY_MAX: usize = 64 << 10;

/// A success reply that is read whole (a model list, a batch of embeddings) is refused above
/// this many bytes.
pub(crate) const REPLY_BODY_MAX: usize = 16 << 20;

/// A non-success status, or an HTML page served as 200: what the codec classifies.
pub(crate) fn is_failure(head: &ResponseHead) -> bool {
    !(200..300).contains(&head.status.0) || head.body == BodyKind::Html
}

/// What the body did against its cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Within,
    Capped,
}

/// Keeps the head and the body, up to a cap that depends on the head.
#[derive(Debug)]
pub(crate) struct Collect {
    head: Option<ResponseHead>,
    body: Vec<u8>,
    cap: usize,
    fill: Fill,
}

impl Default for Collect {
    fn default() -> Self {
        Self {
            head: None,
            body: Vec::new(),
            cap: REPLY_BODY_MAX,
            fill: Fill::Within,
        }
    }
}

impl BodySink for Collect {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.cap = if is_failure(head) {
            ERROR_BODY_MAX
        } else {
            REPLY_BODY_MAX
        };
        self.head = Some(head.clone());
        ChunkFlow::Continue
    }

    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        let room = self.cap.saturating_sub(self.body.len());
        self.body.extend_from_slice(&bytes[..bytes.len().min(room)]);
        if bytes.len() > room {
            self.fill = Fill::Capped;
            return ChunkFlow::Stop;
        }
        ChunkFlow::Continue
    }
}

impl Collect {
    /// The reply, or the error it means. A failing head is classified whatever the transport
    /// said (it reports `Rejected`, which carries nothing); a success needs a transport that
    /// finished, and a body within the cap.
    pub(crate) fn into_reply(
        self,
        codec: &impl ErrorWire,
        result: Result<HttpStatus, HttpError>,
    ) -> Result<(ResponseHead, Vec<u8>), ProviderError> {
        let Some(head) = self.head else {
            return Err(result.err().map_or_else(
                || ProviderError::Unreadable("no response".into()),
                http_error,
            ));
        };
        if is_failure(&head) {
            let error = codec.classify(&head, &self.body);
            return Err(with_retry_after(error, Some(&head)));
        }
        match (result, self.fill) {
            (Err(error), _) => Err(http_error(error)),
            (Ok(_), Fill::Capped) => {
                Err(ProviderError::Unreadable("the reply is too large".into()))
            }
            (Ok(_), Fill::Within) => Ok((head, self.body)),
        }
    }
}
