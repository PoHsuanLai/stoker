//! Reading catalog files.

use model_provider::Caps;
use speech_provider::SpeechDir;

use crate::{CatalogKind, EngineKind, GpuNeed, ModelEntry, WeightFiles};

/// Why a catalog file was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    /// The text is not TOML, or a field is missing or of the wrong type. Carries the parser's
    /// message, which names the field.
    #[error("the file does not parse: {0}")]
    Toml(String),
    #[error("the entry lists no engine")]
    NoEngine,
    #[error("the entry's context window is larger than its maximum output allows for")]
    OutputExceedsContext,
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
    #[error("a vLLM engine needs a GPU memory estimate; the entry's is all zeros")]
    VllmWithoutVram,
    #[error("a llama-server engine reads GGUF files; the profile's weights are not `gguf`")]
    LlamaServerWithoutGguf,
    #[error("the `embed` table needs the embeddings role")]
    EmbedTableWithoutEmbeddingsRole,
}

/// Parses one `catalog/<id>.toml`.
///
/// Beyond the file format: the chat fields are required when a chat role is listed (and refused
/// when none is), the `speech` table when a speech role is, in the direction the role says; one
/// entry has one speech direction; a vLLM engine needs a GPU estimate; a llama-server profile
/// names GGUF weights (`command` has no `--model` for anything else); the `embed` table goes with
/// the `embeddings` role, and an entry that has it needs no chat fields for that role.
pub fn parse_entry(text: &str) -> Result<ModelEntry, CatalogError> {
    let entry: ModelEntry = toml::from_str(text).map_err(|e| CatalogError::Toml(e.to_string()))?;
    if entry.engines.is_empty() {
        return Err(CatalogError::NoEngine);
    }
    if entry.roles.is_empty() {
        return Err(CatalogError::NoRoles);
    }
    check_speech(&entry)?;
    check_chat(&entry, text)?;
    if entry.vram.gpu_need() == GpuNeed::Absent
        && entry.engines.iter().any(|e| e.kind == EngineKind::Vllm)
    {
        return Err(CatalogError::VllmWithoutVram);
    }
    check_weights(&entry)?;
    Ok(entry)
}

fn check_weights(entry: &ModelEntry) -> Result<(), CatalogError> {
    let bad = entry.engines.iter().any(|e| {
        e.kind == EngineKind::LlamaServer && !matches!(e.weights, WeightFiles::Gguf { .. })
    });
    if bad {
        Err(CatalogError::LlamaServerWithoutGguf)
    } else {
        Ok(())
    }
}

fn check_speech(entry: &ModelEntry) -> Result<(), CatalogError> {
    let wanted = match (
        entry.roles.contains(&CatalogKind::SpeechIn),
        entry.roles.contains(&CatalogKind::SpeechOut),
    ) {
        (true, true) => return Err(CatalogError::BothSpeechDirections),
        (true, false) => Some((CatalogKind::SpeechIn, SpeechDir::In)),
        (false, true) => Some((CatalogKind::SpeechOut, SpeechDir::Out)),
        (false, false) => None,
    };
    match (wanted, &entry.speech) {
        (Some(_), None) => Err(CatalogError::SpeechRoleWithoutSpeechTable),
        (None, Some(_)) => Err(CatalogError::SpeechTableWithoutSpeechRole),
        (Some((role, dir)), Some(caps)) if caps.dir() != dir => {
            Err(CatalogError::SpeechDirectionMismatch { role })
        }
        _ => Ok(()),
    }
}

/// Chat fields are needed by every non-speech role, except `embeddings` when the entry has its
/// `embed` table (the table is what that role needs); they are refused when no such role is listed.
fn check_chat(entry: &ModelEntry, text: &str) -> Result<(), CatalogError> {
    let embeddings = entry.roles.contains(&CatalogKind::Embeddings);
    if entry.embed.is_some() && !embeddings {
        return Err(CatalogError::EmbedTableWithoutEmbeddingsRole);
    }
    let chat_role = entry.roles.iter().any(|r| r.speech_dir().is_none());
    let chat_needed = entry.roles.iter().any(|r| {
        r.speech_dir().is_none() && !(*r == CatalogKind::Embeddings && entry.embed.is_some())
    });
    match (chat_role, &entry.caps) {
        (false, _) if entry.sampling.is_some() => Err(CatalogError::SamplingWithoutChatRole),
        (false, Some(_)) => Err(CatalogError::ChatFieldsWithoutChatRole),
        (false, None) => Ok(()),
        (true, None) if !chat_needed => Ok(()),
        (true, caps) => {
            // The flattened field swallows an error, so read the fields again to name the
            // one that is missing.
            let read: Caps = toml::from_str(text).map_err(|e| CatalogError::Toml(e.to_string()))?;
            let caps = caps.as_ref().unwrap_or(&read);
            if caps.max_output > caps.context {
                return Err(CatalogError::OutputExceedsContext);
            }
            if entry.sampling.is_none() && chat_needed {
                return Err(CatalogError::ChatRoleWithoutSampling);
            }
            Ok(())
        }
    }
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
