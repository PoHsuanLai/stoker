//! The cassette file: JSON Lines, `<name>.cassette.jsonl`, a header line then one line per
//! interaction.

use model_provider::{ModelName, ProviderError, Tokens, TurnEnd, TurnEvent};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use speech_provider::SpeechCaps;

use crate::{PrintHash, RequestPrint, check_sequence};

/// The format version of a cassette file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CassetteVersion(pub u32);

impl CassetteVersion {
    pub const CURRENT: CassetteVersion = CassetteVersion(1);
}

/// Seconds since the Unix epoch, UTC, when the cassette was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecordedAt(pub i64);

/// Which engine produced the interactions (`vllm`, `llama_server`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineLabel(pub String);

/// The label of a backend before the header carried a stamp.
pub type BackendLabel = EngineLabel;

/// The build of the engine (`b10964-b29c606e2`): a fixture is only as good as the build it came
/// from, and a replay that fails says which build to record again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildLabel(pub String);

/// What a cassette was recorded from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineStamp {
    pub kind: EngineLabel,
    pub build: BuildLabel,
}

/// The context sizes the recorded model reported (llama.cpp `n_ctx`, vLLM `max_model_len`, and
/// the trained context), so a replayed `describe` answers what the engine did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextStamp {
    pub loaded: Tokens,
    pub trained: Tokens,
}

/// The header of every cassette file, chat, speech and wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CassetteHeader {
    pub vocab: CassetteVersion,
    pub engine: EngineStamp,
    pub model: ModelName,
    pub recorded: RecordedAt,
    /// What the engine reported for the model's context; `describe` of a replay answers it.
    pub context: ContextStamp,
    /// What the speech model could do, for a speech cassette; `None` for a chat or wire one, and
    /// for a speech cassette whose replay infers the caps from the recorded calls.
    pub speech: Option<SpeechCaps>,
}

/// The index of an interaction in its cassette, from 0: what a `Mismatch` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InteractionId(pub u32);

/// One turn: the request's fingerprint, the events it produced, how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    pub id: InteractionId,
    pub request: RequestPrint,
    /// `request.hash()`, stored beside the readable print so `ByRequest` matches in O(1) and
    /// tolerates field order.
    pub print: PrintHash,
    pub events: Vec<TurnEvent>,
    pub end: Result<TurnEnd, ProviderError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cassette {
    pub header: CassetteHeader,
    pub interactions: Vec<Interaction>,
}

/// A cassette file that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CassetteError {
    #[error("the file is empty")]
    Empty,
    #[error("line {line} does not parse")]
    BadLine { line: u32 },
    /// The events of an interaction are not a possible stream (`check_sequence`).
    #[error("line {line} holds events that are not a possible stream")]
    BadSequence { line: u32 },
    #[error("the cassette is version {found:?}, this build reads {want:?}")]
    Version {
        found: CassetteVersion,
        want: CassetteVersion,
    },
}

impl Cassette {
    /// The file's text: the header, then one line per interaction.
    pub fn to_jsonl(&self) -> String {
        write_jsonl(&self.header, &self.interactions)
    }

    pub fn from_jsonl(text: &str) -> Result<Cassette, CassetteError> {
        let (header, interactions) = read_jsonl(text)?;
        let cassette = Cassette {
            header,
            interactions,
        };
        cassette.check_sequences()?;
        Ok(cassette)
    }

    /// Runs `check_sequence` over every interaction's events: `BadSequence` names the line
    /// (the header is line 1). `from_jsonl` calls it once the file is built.
    pub fn check_sequences(&self) -> Result<(), CassetteError> {
        self.interactions
            .iter()
            .enumerate()
            .try_for_each(|(at, interaction)| {
                check_sequence(&interaction.events).map_err(|_| CassetteError::BadSequence {
                    line: u32::try_from(at + 2).unwrap_or(u32::MAX),
                })
            })
    }
}

/// The JSON Lines text of a header and its lines, shared by the chat and speech cassettes.
pub(crate) fn write_jsonl<I: Serialize>(header: &CassetteHeader, lines: &[I]) -> String {
    let mut out = String::new();
    // Serialising these types cannot fail: no map has non-string keys.
    out.push_str(&serde_json::to_string(header).unwrap_or_default());
    out.push('\n');
    for line in lines {
        out.push_str(&serde_json::to_string(line).unwrap_or_default());
        out.push('\n');
    }
    out
}

pub(crate) fn read_jsonl<I: DeserializeOwned>(
    text: &str,
) -> Result<(CassetteHeader, Vec<I>), CassetteError> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let (_, first) = lines.next().ok_or(CassetteError::Empty)?;
    let header: CassetteHeader =
        serde_json::from_str(first).map_err(|_| CassetteError::BadLine { line: 1 })?;
    if header.vocab != CassetteVersion::CURRENT {
        return Err(CassetteError::Version {
            found: header.vocab,
            want: CassetteVersion::CURRENT,
        });
    }
    let items = lines
        .map(|(at, line)| {
            serde_json::from_str(line).map_err(|_| CassetteError::BadLine {
                line: u32::try_from(at + 1).unwrap_or(u32::MAX),
            })
        })
        .collect::<Result<Vec<I>, _>>()?;
    Ok((header, items))
}
