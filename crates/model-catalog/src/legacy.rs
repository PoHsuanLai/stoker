//! Files in the older shape: `roles`, the chat fields at the top level, a `speech` table, an
//! `embed` table. They still load (a user's catalogue directory may hold them); the roles and
//! chat fields are kept as written, and the capabilities are converted from them.

use std::collections::BTreeSet;

use model_provider::{Caps, CuaSupport, EmbedCaps, InputKind};
use serde::Deserialize;
use speech_provider::{SpeechCaps, SpeechDir};

use crate::{
    Capabilities, CatalogError, CatalogId, CatalogKind, ColdStartEstimateS, EngineProfile, Family,
    Licence, Locality, Modalities, Modality, ModelEntry, SamplingDefaults, TextOut, VramEstimate,
    WeightSource,
};

/// The older file's serde form.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LegacyFile {
    id: CatalogId,
    label: String,
    licence: Licence,
    family: Family,
    cold_start_estimate_s: ColdStartEstimateS,
    source: WeightSource,
    vram: VramEstimate,
    roles: BTreeSet<CatalogKind>,
    #[serde(flatten)]
    caps: Option<Caps>,
    sampling: Option<SamplingDefaults>,
    speech: Option<SpeechCaps>,
    embed: Option<EmbedCaps>,
    #[serde(rename = "engine")]
    engines: Vec<EngineProfile>,
}

/// Reads and checks an older-shape file.
pub fn parse(text: &str) -> Result<ModelEntry, CatalogError> {
    let file: LegacyFile = toml::from_str(text).map_err(|e| CatalogError::Toml(e.to_string()))?;
    if file.engines.is_empty() {
        return Err(CatalogError::NoEngine);
    }
    if file.roles.is_empty() {
        return Err(CatalogError::NoRoles);
    }
    check_speech(&file)?;
    check_chat(&file, text)?;
    Ok(file.into_entry())
}

impl LegacyFile {
    fn into_entry(self) -> ModelEntry {
        ModelEntry {
            capabilities: self.capabilities(),
            id: self.id,
            label: self.label,
            licence: self.licence,
            family: self.family,
            cold_start_estimate_s: self.cold_start_estimate_s,
            source: self.source,
            vram: self.vram,
            locality: Locality::OnDevice,
            roles: self.roles,
            caps: self.caps,
            sampling: self.sampling,
            speech: self.speech,
            embed: self.embed,
            engines: self.engines,
        }
    }

    /// What the file's fields say in the new terms: chat caps give text out (and image in when
    /// the model takes images), a computer-use role with a dialect gives actions out, the embed
    /// table gives vector out, a speech table gives audio in or out. A speech-to-text model also
    /// outputs text, with a text table that says it has no chat window.
    fn capabilities(&self) -> Capabilities {
        let mut inputs = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        let mut c = Capabilities {
            inputs: Modalities::default(),
            outputs: Modalities::default(),
            text_out: None,
            image_in: None,
            audio_in: None,
            audio_out: None,
            vector_out: None,
            actions_out: None,
        };
        if let Some(caps) = &self.caps {
            outputs.insert(Modality::Text);
            c.text_out = Some(TextOut {
                tools: caps.tools,
                structured: caps.output.clone(),
                reasoning: caps.reasoning,
                streaming: caps.streaming,
                context: caps.context,
                max_output: caps.max_output,
                sampling: self.sampling,
            });
            for kind in &caps.inputs {
                inputs.insert(match kind {
                    InputKind::Text => Modality::Text,
                    InputKind::Image => Modality::Image,
                });
            }
            let actions = self.roles.contains(&CatalogKind::ComputerUse)
                && caps.computer_use != CuaSupport::Absent;
            if actions {
                outputs.insert(Modality::Actions);
                inputs.insert(Modality::Image);
                c.actions_out = Some(caps.computer_use);
            }
            if inputs.contains(&Modality::Image) {
                c.image_in = Some(caps.images);
            }
        }
        if let Some(embed) = &self.embed {
            inputs.insert(Modality::Text);
            outputs.insert(Modality::Vector);
            c.vector_out = Some(embed.clone());
        }
        match &self.speech {
            Some(speech) if speech.dir() == SpeechDir::In => {
                inputs.insert(Modality::Audio);
                outputs.insert(Modality::Text);
                c.text_out.get_or_insert_with(|| transcriber_text(speech));
                c.audio_in = Some(speech.clone());
            }
            Some(speech) => {
                inputs.insert(Modality::Text);
                outputs.insert(Modality::Audio);
                c.audio_out = Some(speech.clone());
            }
            None => {}
        }
        c.inputs = Modalities(inputs);
        c.outputs = Modalities(outputs);
        c
    }
}

/// The text side of a model that only transcribes: no tools, no window.
fn transcriber_text(speech: &SpeechCaps) -> TextOut {
    use model_provider::{Support, Tokens, ToolSupport};
    TextOut {
        tools: ToolSupport::Absent,
        structured: BTreeSet::new(),
        reasoning: Support::Absent,
        streaming: speech.streaming,
        context: Tokens(0),
        max_output: Tokens(0),
        sampling: None,
    }
}

fn check_speech(file: &LegacyFile) -> Result<(), CatalogError> {
    let wanted = match (
        file.roles.contains(&CatalogKind::SpeechIn),
        file.roles.contains(&CatalogKind::SpeechOut),
    ) {
        (true, true) => return Err(CatalogError::BothSpeechDirections),
        (true, false) => Some((CatalogKind::SpeechIn, SpeechDir::In)),
        (false, true) => Some((CatalogKind::SpeechOut, SpeechDir::Out)),
        (false, false) => None,
    };
    match (wanted, &file.speech) {
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
fn check_chat(file: &LegacyFile, text: &str) -> Result<(), CatalogError> {
    let embeddings = file.roles.contains(&CatalogKind::Embeddings);
    if file.embed.is_some() && !embeddings {
        return Err(CatalogError::EmbedTableWithoutEmbeddingsRole);
    }
    let chat_role = file.roles.iter().any(|r| r.speech_dir().is_none());
    let chat_needed = file.roles.iter().any(|r| {
        r.speech_dir().is_none() && !(*r == CatalogKind::Embeddings && file.embed.is_some())
    });
    match (chat_role, &file.caps) {
        (false, _) if file.sampling.is_some() => Err(CatalogError::SamplingWithoutChatRole),
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
            if file.sampling.is_none() && chat_needed {
                return Err(CatalogError::ChatRoleWithoutSampling);
            }
            Ok(())
        }
    }
}
