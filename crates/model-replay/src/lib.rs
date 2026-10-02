//! Cassettes of model turns: record once from a real engine by a dev script, replay in tests.
//!
//! By default nothing personal is recorded: images and audio are stored as digests, and recording
//! is a dev-script action, never automatic. Speech cassettes (`speech`) share the file format and
//! the header with the chat ones.

mod cassette;
mod print;
mod provider;
mod speech;

pub use cassette::{
    BackendLabel, Cassette, CassetteError, CassetteHeader, CassetteVersion, Interaction, RecordedAt,
};
pub use print::{
    ByteCount, ImageDigest, ImagePrint, MessagePrint, PartPrint, RequestPrint, ToolResultPrint,
};
pub use provider::{
    CassetteSink, RecordingProvider, ReplayError, ReplayMode, ReplayProvider, SinkError,
};
pub use speech::{
    AudioDigest, AudioPrint, RecordingSpeech, SpeechCassette, SpeechCassetteSink,
    SpeechInteraction, SpeechReplay, SttPrint, TextDigest, TextPrint, TtsPrint,
};
