//! Cassettes of model turns: record once from a real engine by a dev script, replay in tests.
//!
//! By default nothing personal is recorded: images and audio are stored as digests, and recording
//! is a dev-script action, never automatic. Speech cassettes (`speech`) and wire cassettes (`wire`,
//! HTTP exchanges recorded at the `Transport` seam) share the file format and the header with the
//! chat ones.

mod canon;
mod cassette;
mod print;
mod provider;
mod sequence;
mod speech;
mod wire;
mod wire_form;
mod wire_record;
mod wire_replay;

pub use cassette::{
    BackendLabel, BuildLabel, Cassette, CassetteError, CassetteHeader, CassetteVersion,
    EngineLabel, EngineStamp, Interaction, InteractionId, RecordedAt,
};
pub use print::{
    ByteCount, ImageDigest, ImagePrint, MessagePrint, PartPrint, PrintHash, RequestPrint,
    ToolResultPrint,
};
pub use provider::{
    CassetteSink, RecordingProvider, ReplayError, ReplayMode, ReplayProvider, SinkError,
};
pub use sequence::{StreamFault, check_sequence};
pub use speech::{
    AudioDigest, AudioPrint, RecordingSpeech, SpeechCassette, SpeechCassetteSink,
    SpeechInteraction, SpeechReplay, SttPrint, TextDigest, TextPrint, TtsPrint,
};
pub use wire::{
    ByteStep, ChunkPlan, HeadPrint, WireBody, WireCassette, WireEnd, WireExchange, WireFrame,
    WireHeader, WireMiss, WireReply, WireRequest, WireSink,
};
pub use wire_record::RecordingTransport;
pub use wire_replay::ReplayTransport;
