//! Cassettes of model turns: record once from a real engine by a dev script, replay in tests.
//!
//! By default nothing personal is recorded: images are stored as digests, and recording is a
//! dev-script action, never automatic.

mod cassette;
mod print;
mod provider;

pub use cassette::{
    BackendLabel, Cassette, CassetteError, CassetteHeader, CassetteVersion, Interaction, RecordedAt,
};
pub use print::{
    ByteCount, ImageDigest, ImagePrint, MessagePrint, PartPrint, RequestPrint, ToolResultPrint,
};
pub use provider::{
    CassetteSink, RecordingProvider, ReplayError, ReplayMode, ReplayProvider, SinkError,
};
