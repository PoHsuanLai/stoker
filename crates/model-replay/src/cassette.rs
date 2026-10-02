//! The cassette file: JSON Lines, `<name>.cassette.jsonl`, a header line then one line per
//! interaction.

use model_provider::{ModelName, ProviderError, TurnEnd, TurnEvent};
use serde::{Deserialize, Serialize};

use crate::RequestPrint;

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

/// Which backend produced the interactions (`vllm`, `llama_server`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackendLabel(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CassetteHeader {
    pub vocab: CassetteVersion,
    pub backend: BackendLabel,
    pub model: ModelName,
    pub recorded: RecordedAt,
}

/// One turn: the request's fingerprint, the events it produced, how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    pub request: RequestPrint,
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
    #[error("the cassette is version {found:?}, this build reads {want:?}")]
    Version {
        found: CassetteVersion,
        want: CassetteVersion,
    },
}

impl Cassette {
    /// The file's text: the header, then one line per interaction.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        // Serialising these types cannot fail: no map has non-string keys.
        out.push_str(&serde_json::to_string(&self.header).unwrap_or_default());
        out.push('\n');
        for interaction in &self.interactions {
            out.push_str(&serde_json::to_string(interaction).unwrap_or_default());
            out.push('\n');
        }
        out
    }

    pub fn from_jsonl(text: &str) -> Result<Cassette, CassetteError> {
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
        let interactions = lines
            .map(|(at, line)| {
                serde_json::from_str(line).map_err(|_| CassetteError::BadLine {
                    line: u32::try_from(at + 1).unwrap_or(u32::MAX),
                })
            })
            .collect::<Result<Vec<Interaction>, _>>()?;
        Ok(Cassette {
            header,
            interactions,
        })
    }
}
