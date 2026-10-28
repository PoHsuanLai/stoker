//! Reading catalog files.

use crate::CatalogKind;
use crate::file::EntryFile;
use crate::{DetailTable, Locality, Modality, ModelEntry, Serving, check, legacy};

/// Why a catalog file was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// The text is not TOML, or a field is missing or of the wrong type. Carries the parser's
    /// message, which names the field.
    #[error("the file does not parse: {0}")]
    Toml(String),
    #[error("the entry lists no engine")]
    NoEngine,
    #[error("the entry's context window is larger than its maximum output allows for")]
    OutputExceedsContext,
    #[error("the entry's served context window is larger than the model's maximum")]
    ContextExceedsModelMax,
    #[error("the entry's served maximum output is larger than the model's maximum output")]
    OutputExceedsModelMax,
    #[error("the entry's minimum context is larger than the model's maximum")]
    MinimumExceedsModelMax,
    #[error("the entry lists no role")]
    NoRoles,
    #[error("the entry lists both speech_in and speech_out; a speech table has one direction")]
    BothSpeechDirections,
    #[error("a speech role needs the `speech` table")]
    SpeechRoleWithoutSpeechTable,
    #[error("the `speech` table needs a speech_in or speech_out role")]
    SpeechTableWithoutSpeechRole,
    #[error("role {role:?} does not match the direction of the `speech` table")]
    SpeechDirectionMismatch { role: CatalogKind },
    #[error("the entry has chat capability fields but no chat role")]
    ChatFieldsWithoutChatRole,
    #[error("a chat role needs the `sampling` table")]
    ChatRoleWithoutSampling,
    #[error("the `sampling` table needs a chat role")]
    SamplingWithoutChatRole,
    #[error("the `embed` table needs the embeddings role")]
    EmbedTableWithoutEmbeddingsRole,
    #[error("a remote entry runs on the providers and lists no engine")]
    RemoteWithEngines,
    #[error("a remote entry lists no way to reach it")]
    RemoteWithoutReach,
    #[error("an attached entry is served elsewhere and lists no engine")]
    AttachedWithEngines,
    #[error("an attached entry is on-device: it has no reach")]
    AttachedRemote,
    #[error("the entry takes no input")]
    NoInputs,
    #[error("the entry gives no output")]
    NoOutputs,
    #[error("{modality:?} is an output only; no model takes it as input")]
    OutputOnlyModalityAsInput { modality: Modality },
    #[error("the `{table:?}` table is written but its modality is not declared")]
    TableWithoutModality { table: DetailTable },
    #[error("the modality of the `{table:?}` table is declared but the table is missing")]
    ModalityWithoutTable { table: DetailTable },
    #[error("the `{table:?}` table has the wrong speech direction")]
    AudioTableDirection { table: DetailTable },
    #[error("computer-use output needs text and image input and the `text_out` table")]
    ActionsNeedTextAndImage,
    #[error(
        "`tools = \"client_parsed\"` and a computer-use `calls` format in the action profile go together"
    )]
    ClientParsedWithoutCallFormat,
    #[error(
        "an attached engine names its tool parser unless the entry's calls are read from the text (`tools = \"client_parsed\"`), and then it names none"
    )]
    AttachedToolParser,
    #[error("engine {engine} passes more inputs or outputs than its model has")]
    EngineWidensModel { engine: usize },
}

/// Parses one `catalog/<id>.toml`.
///
/// Beyond the file format: the declared inputs and outputs and the detail tables agree (see
/// `check::capabilities`) and an engine's subset stays inside its model's sets. What an entry
/// needs of its engines (a GPU estimate, weights the engine reads) is `engine_spec::check_entry`,
/// which the caller runs next. A file with a `roles` key is in the older shape and is read by
/// `legacy`.
pub fn parse_entry(text: &str) -> Result<ModelEntry, CatalogError> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| CatalogError::Toml(e.to_string()))?;
    let entry = match table.contains_key("roles") {
        true => legacy::parse(text)?,
        false => parse_new(text)?,
    };
    Ok(entry)
}

fn parse_new(text: &str) -> Result<ModelEntry, CatalogError> {
    let file: EntryFile = toml::from_str(text).map_err(|e| CatalogError::Toml(e.to_string()))?;
    match (&file.serving, &file.locality, file.engines.is_empty()) {
        (Serving::Attached(_), Locality::Remote { .. }, _) => {
            return Err(CatalogError::AttachedRemote);
        }
        (Serving::Attached(_), Locality::OnDevice, false) => {
            return Err(CatalogError::AttachedWithEngines);
        }
        (Serving::Attached(_), Locality::OnDevice, true) => {}
        (Serving::Launched, Locality::OnDevice, true) => return Err(CatalogError::NoEngine),
        (Serving::Launched, Locality::Remote { .. }, false) => {
            return Err(CatalogError::RemoteWithEngines);
        }
        (Serving::Launched, Locality::Remote { reach }, true) if reach.is_empty() => {
            return Err(CatalogError::RemoteWithoutReach);
        }
        _ => {}
    }
    let capabilities = file.capabilities();
    check::capabilities(&capabilities, &file.locality)?;
    check::attached_parser(&capabilities, &file.serving)?;
    check::narrowing(&capabilities, &file.engines)?;
    Ok(file.into_entry())
}

/// The system entries with the user's applied: a user entry replaces the system entry of the
/// same id, and a user entry with a new id is added. Order: system first, then new user ids.
pub fn merge_catalogs(system: Vec<ModelEntry>, user: Vec<ModelEntry>) -> Vec<ModelEntry> {
    let mut merged: Vec<ModelEntry> = system
        .into_iter()
        .map(|entry| {
            user.iter()
                .find(|u| u.id == entry.id)
                .cloned()
                .unwrap_or(entry)
        })
        .collect();
    let added: Vec<ModelEntry> = user
        .into_iter()
        .filter(|u| !merged.iter().any(|m| m.id == u.id))
        .collect();
    merged.extend(added);
    merged
}
