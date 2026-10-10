//! A model's declared capabilities: what it takes in, what it gives out, and one detail table
//! for each modality that needs one. The table is there exactly when its modality is.

use std::collections::BTreeSet;

use model_provider::{
    Constraint, CuaSupport, EmbedCaps, ImageLimits, Support, Tokens, ToolSupport,
};
use serde::{Deserialize, Serialize};
use speech_provider::SpeechCaps;

use crate::{EngineProfile, Modalities, Modality, SamplingDefaults};

/// Detail for `text` as an output: what the text side of a model can do.
///
/// A model that only transcribes (Whisper) still outputs text; it has no chat window, so its
/// `context` and `max_output` are zero and its audio limits sit in `audio_in`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextOut {
    pub tools: ToolSupport,
    /// The output constraints the engine can enforce (structured output).
    pub structured: BTreeSet<Constraint>,
    pub reasoning: Support,
    pub streaming: Support,
    pub context: Tokens,
    pub max_output: Tokens,
    /// Written exactly when text is also an input (a chat model); `parse_entry` checks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingDefaults>,
}

/// The detail tables a modality needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DetailTable {
    TextOut,
    ImageIn,
    AudioIn,
    AudioOut,
    VectorOut,
    ActionsOut,
}

/// Which side of a model a modality sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    In,
    Out,
}

impl DetailTable {
    /// The side and modality that make the table required.
    pub fn needs(self) -> (Side, Modality) {
        match self {
            DetailTable::TextOut => (Side::Out, Modality::Text),
            DetailTable::ImageIn => (Side::In, Modality::Image),
            DetailTable::AudioIn => (Side::In, Modality::Audio),
            DetailTable::AudioOut => (Side::Out, Modality::Audio),
            DetailTable::VectorOut => (Side::Out, Modality::Vector),
            DetailTable::ActionsOut => (Side::Out, Modality::Actions),
        }
    }
}

/// What a model takes in and gives out, with the detail tables for those modalities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub inputs: Modalities,
    pub outputs: Modalities,
    pub text_out: Option<TextOut>,
    pub image_in: Option<ImageLimits>,
    /// A speech table in the `in` direction.
    pub audio_in: Option<SpeechCaps>,
    /// A speech table in the `out` direction.
    pub audio_out: Option<SpeechCaps>,
    pub vector_out: Option<EmbedCaps>,
    /// Never `CuaSupport::Absent`: no actions output, no table.
    pub actions_out: Option<CuaSupport>,
}

impl Capabilities {
    /// Whether the table of `table` is written.
    pub fn has_table(&self, table: DetailTable) -> bool {
        match table {
            DetailTable::TextOut => self.text_out.is_some(),
            DetailTable::ImageIn => self.image_in.is_some(),
            DetailTable::AudioIn => self.audio_in.is_some(),
            DetailTable::AudioOut => self.audio_out.is_some(),
            DetailTable::VectorOut => self.vector_out.is_some(),
            DetailTable::ActionsOut => self.actions_out.is_some(),
        }
    }

    /// Whether the modality a table belongs to is declared on the table's side.
    pub fn declares(&self, table: DetailTable) -> bool {
        match table.needs() {
            (Side::In, modality) => self.inputs.contains(modality),
            (Side::Out, modality) => self.outputs.contains(modality),
        }
    }

    /// What this model offers on one engine: the declared sets narrowed to what the profile
    /// passes through, and the tables of dropped modalities dropped with them. A profile that
    /// names no subset passes everything. Pure; a profile that names more than the model has
    /// is refused by the parser, and here would only intersect.
    pub fn on_engine(&self, profile: &EngineProfile) -> Capabilities {
        let inputs = match &profile.inputs {
            Some(passed) => self.inputs.intersection(passed),
            None => self.inputs.clone(),
        };
        let outputs = match &profile.outputs {
            Some(passed) => self.outputs.intersection(passed),
            None => self.outputs.clone(),
        };
        let mut narrowed = Capabilities {
            inputs,
            outputs,
            ..self.clone()
        };
        let keep = |table: DetailTable| narrowed.declares(table);
        let (text, image, audio_in, audio_out, vector, actions) = (
            keep(DetailTable::TextOut),
            keep(DetailTable::ImageIn),
            keep(DetailTable::AudioIn),
            keep(DetailTable::AudioOut),
            keep(DetailTable::VectorOut),
            keep(DetailTable::ActionsOut),
        );
        narrowed.text_out = narrowed.text_out.take().filter(|_| text);
        narrowed.image_in = narrowed.image_in.take().filter(|_| image);
        narrowed.audio_in = narrowed.audio_in.take().filter(|_| audio_in);
        narrowed.audio_out = narrowed.audio_out.take().filter(|_| audio_out);
        narrowed.vector_out = narrowed.vector_out.take().filter(|_| vector);
        narrowed.actions_out = narrowed.actions_out.take().filter(|_| actions);
        narrowed
    }
}
