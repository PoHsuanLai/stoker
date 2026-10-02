//! The speech-to-text engine process.
//!
//! inferd starts it as an engine unit (catalog kind `speech_host`) with the arguments of the
//! entry's profile; it listens on a Unix socket and speaks `speech_provider::host_wire`: a
//! `Hello` exchange, then `Begin`, `Audio`, `End` or `Cancel` in, `Event`, `Done` or `Failed`
//! out. One utterance at a time. It never opens a device, a network socket or a file it was not
//! given, and it writes nothing to disk: audio lives in memory for the utterance.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use speech_provider::AudioMs;

/// Threads the recognizer may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadCount(pub u16);

/// What the command line says: `--socket <path> --model-dir <dir> --threads <n> --chunk-ms <ms>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostArgs {
    pub socket: PathBuf,
    /// The directory of ONNX files and `tokens.txt`.
    pub model_dir: PathBuf,
    pub threads: ThreadCount,
    pub chunk: AudioMs,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("missing argument {0}")]
    Missing(&'static str),
    #[error("argument {0} is not a valid value")]
    Invalid(&'static str),
    #[error("unknown argument {0}")]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("the socket cannot be bound")]
    Bind,
    #[error("the model directory does not load")]
    Model,
}

/// Reads the arguments after the program name.
pub fn parse_args(args: &[String]) -> Result<HostArgs, ArgsError> {
    let _ = args;
    todo!("parse_args: the four flags, each once, nothing else")
}

/// Binds the socket and serves utterances until the process is stopped.
pub fn serve(args: &HostArgs) -> Result<(), HostError> {
    let _ = args;
    todo!("serve: sherpa-onnx online recognizer, one utterance at a time over host_wire")
}
