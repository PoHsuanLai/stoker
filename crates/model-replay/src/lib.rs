//! Cassettes of model turns: record once from a real engine by a dev script, replay in tests.
//!
//! By default nothing personal is recorded: images and audio are stored as digests, and recording
//! is a dev-script action, never automatic. Speech cassettes (`speech`) and wire cassettes (`wire`,
//! HTTP exchanges recorded at the `Transport` seam) share the file format and the header with the
//! chat ones.
//!
//! ```no_run
//! use model_replay::{Cassette, ReplayMode, ReplayProvider};
//!
//! // A cassette recorded earlier by a dev script; `from_jsonl` also checks every stream.
//! let text = std::fs::read_to_string("tests/cassettes/greeting.jsonl")?;
//! let cassette = Cassette::from_jsonl(&text)?;
//!
//! // The nth turn of the test gets the nth recorded interaction.
//! let provider = ReplayProvider::new(cassette, ReplayMode::InOrder);
//! // ... run the code under test with `provider` as its `Provider` ...
//! assert!(provider.misses().is_empty());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

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
    ContextStamp, EngineLabel, EngineStamp, Interaction, InteractionId, RecordedAt,
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
