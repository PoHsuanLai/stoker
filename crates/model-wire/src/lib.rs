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

mod codec;
mod collect;
mod driver;
mod errors;
mod sink;

pub use codec::{ChatCodec, ChatDecoder, CodecError, EmbedCodec, ErrorWire};
pub use driver::Driver;
