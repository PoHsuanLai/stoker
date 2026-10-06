//! The catalog file: the entry's serde form. Read by `parse_entry`, written by `ModelEntry`'s
//! `Serialize`.

use model_provider::{CuaSupport, EmbedCaps, ImageLimits};
use serde::{Deserialize, Serialize, Serializer};
use speech_provider::SpeechCaps;

use crate::view::derive;
use crate::{
    Capabilities, CatalogId, ColdStartEstimateS, EngineProfile, Family, Licence, Modalities,
    ModelEntry, TextOut, VramEstimate, WeightSource,
};

/// One `catalog/<id>.toml`: the header, what goes in and out, the detail table of each declared
/// modality, and the engine profiles. A key this struct does not name is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryFile {
    pub id: CatalogId,
    pub label: String,
    pub licence: Licence,
    pub family: Family,
    pub cold_start_estimate_s: ColdStartEstimateS,
    pub source: WeightSource,
    pub vram: VramEstimate,
    pub inputs: Modalities,
    pub outputs: Modalities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_out: Option<TextOut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_in: Option<ImageLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_in: Option<SpeechCaps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_out: Option<SpeechCaps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_out: Option<EmbedCaps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_out: Option<CuaSupport>,
    #[serde(rename = "engine")]
    pub engines: Vec<EngineProfile>,
}

impl EntryFile {
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            text_out: self.text_out.clone(),
            image_in: self.image_in,
            audio_in: self.audio_in.clone(),
            audio_out: self.audio_out.clone(),
            vector_out: self.vector_out.clone(),
            actions_out: self.actions_out,
        }
    }

    /// The entry, with the older fields derived from its capabilities.
    pub fn into_entry(self) -> ModelEntry {
        let capabilities = self.capabilities();
        let view = derive(&capabilities);
        ModelEntry {
            id: self.id,
            label: self.label,
            licence: self.licence,
            family: self.family,
            cold_start_estimate_s: self.cold_start_estimate_s,
            source: self.source,
            vram: self.vram,
            capabilities,
            roles: view.roles,
            caps: view.caps,
            sampling: view.sampling,
            speech: view.speech,
            embed: view.embed,
            engines: self.engines,
        }
    }
}

impl From<&ModelEntry> for EntryFile {
    fn from(entry: &ModelEntry) -> Self {
        let c = &entry.capabilities;
        EntryFile {
            id: entry.id.clone(),
            label: entry.label.clone(),
            licence: entry.licence.clone(),
            family: entry.family.clone(),
            cold_start_estimate_s: entry.cold_start_estimate_s,
            source: entry.source.clone(),
            vram: entry.vram,
            inputs: c.inputs.clone(),
            outputs: c.outputs.clone(),
            text_out: c.text_out.clone(),
            image_in: c.image_in,
            audio_in: c.audio_in.clone(),
            audio_out: c.audio_out.clone(),
            vector_out: c.vector_out.clone(),
            actions_out: c.actions_out,
            engines: entry.engines.clone(),
        }
    }
}

/// An entry is written in the file's shape, whatever shape it was read from.
impl Serialize for ModelEntry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EntryFile::from(self).serialize(serializer)
    }
}
