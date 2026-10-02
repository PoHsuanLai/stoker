//! Reading catalog files.

use crate::ModelEntry;

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
}

/// Parses one `catalog/<id>.toml`.
pub fn parse_entry(text: &str) -> Result<ModelEntry, CatalogError> {
    let entry: ModelEntry = toml::from_str(text).map_err(|e| CatalogError::Toml(e.to_string()))?;
    if entry.engines.is_empty() {
        return Err(CatalogError::NoEngine);
    }
    if entry.caps.max_output > entry.caps.context {
        return Err(CatalogError::OutputExceedsContext);
    }
    Ok(entry)
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
