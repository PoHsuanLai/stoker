//! The speech-to-text engine process, protocol half.
//!
//! inferd starts it as an engine unit (catalog kind `speech_host`) with the arguments of the
//! entry's profile; it listens on a Unix socket and speaks `speech_provider::host_wire`: a
//! `Hello` exchange, then `Begin`, `Audio`, `End` or `Cancel` in, `Event`, `Done` or `Failed`
//! out. One utterance at a time; a second `Begin` while one runs is refused and the running one
//! goes on. It never opens a device, a network socket or a file it was not given, and it writes
//! nothing to disk: audio lives in memory for the utterance.
//!
//! This crate has no engine and no native code, so the gate tests the whole loop over a
//! [`Recognizer`] seam. The sherpa-onnx recognizer and the `speech-host` binary live in the
//! excluded sibling crate `speech-host-sherpa`.

mod args;
mod recognizer;
mod serve;
mod session;

#[cfg(test)]
mod tests;

pub use args::{ArgsError, HostArgs, ThreadCount, parse_args};
pub use recognizer::{Finished, Recognizer};
pub use serve::{bind, serve_connection, serve_listener, serve_with};
pub use session::Session;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("the socket cannot be bound")]
    Bind,
    #[error("the model directory does not load")]
    Model,
}

/// Serves with no engine linked: this crate cannot recognise speech, so it answers
/// `HostError::Model`. The binary of `speech-host-sherpa` calls [`serve_with`] with its
/// recognizer instead.
pub fn serve(args: &HostArgs) -> Result<(), HostError> {
    let _ = args;
    Err(HostError::Model)
}
