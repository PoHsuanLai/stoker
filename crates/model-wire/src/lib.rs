//! The wire of a model endpoint, apart from how bytes travel.
//!
//! A [`ChatCodec`] encodes a `TurnRequest` into an `Exchange`, decodes the reply frame by frame
//! with a [`ChatDecoder`], and classifies an error response from its head and body. A
//! `model_http::Transport` delivers the exchange. [`Driver`] joins the two into a `Provider`;
//! `inferd` holds a closed enum of the instantiations. Neither half names the other's types
//! beyond the `Exchange`, the `ResponseHead` and the `BodySink`, so wire fixtures are recorded
//! and replayed at the transport (`model-replay::wire`).
//!
//! Pure over a seam: the crate reaches no HTTP stack and no runtime.
//!
//! ```
//! use model_http::{HttpError, HttpStatus};
//! use model_provider::{ProviderError, ServerStatus};
//! use model_wire::http_error;
//!
//! // A transport failure becomes the error a caller acts on, carrying no body.
//! assert_eq!(http_error(HttpError::Timeout), ProviderError::Timeout);
//! assert_eq!(
//!     http_error(HttpError::Status(HttpStatus(503))),
//!     ProviderError::Server(ServerStatus(503))
//! );
//! ```

mod codec;
mod collect;
mod driver;
mod errors;
mod sink;

pub use codec::{ChatCodec, ChatDecoder, CodecError, EmbedCodec, ErrorWire};
pub use driver::Driver;
pub use errors::http_error;
