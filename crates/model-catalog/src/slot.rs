//! Slots: the jobs a person picks a model for, each a capability signature.
//!
//! A model is listed in every slot its capabilities satisfy, so one entry may be in several. The
//! rules are pure functions of the catalogue; nothing is read from a model's name.
//!
//! Later slots: `image_gen` (text in, image out) and `rerank` stay out until an entry needs them.

use serde::{Deserialize, Serialize};

use model_provider::ToolSupport;

use crate::{Capabilities, EngineKind, Locality, Modality, ModelEntry, Serving};

/// A job with a required capability signature. The slug is the settings key's segment:
/// `ai.model.<slot>.<tier>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    /// Text in, text out.
    Text,
    /// Audio in, text out: speech to text.
    VoiceIn,
    /// Text in, audio out: text to speech.
    VoiceOut,
    /// Image in, text out.
    ImageIn,
    /// Image in, actions out.
    ComputerUse,
    /// Text in, vector out.
    Embeddings,
}

/// Whether a slot needs the model to call tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolNeed {
    Any,
    /// The text side must support tool calls (native or parsed by the engine).
    Required,
}

/// What a slot needs: one modality taken in and one given out (a model may take and give more),
/// and whether it must call tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature {
    pub takes: Modality,
    pub gives: Modality,
    pub tools: ToolNeed,
}

impl Slot {
    /// The slot's capability signature. Only `Text` needs tools: capabilities.md section 2 writes it
    /// "text in -> text out (+ tools)", because the agent loop is built on tool calls; a text model
    /// without them is not offered there. The other slots are one conversion each.
    pub fn signature(self) -> Signature {
        let (takes, gives, tools) = match self {
            Slot::Text => (Modality::Text, Modality::Text, ToolNeed::Required),
            Slot::VoiceIn => (Modality::Audio, Modality::Text, ToolNeed::Any),
            Slot::VoiceOut => (Modality::Text, Modality::Audio, ToolNeed::Any),
            Slot::ImageIn => (Modality::Image, Modality::Text, ToolNeed::Any),
            Slot::ComputerUse => (Modality::Image, Modality::Actions, ToolNeed::Any),
            Slot::Embeddings => (Modality::Text, Modality::Vector, ToolNeed::Any),
        };
        Signature {
            takes,
            gives,
            tools,
        }
    }
}

/// Whether capabilities (usually engine-narrowed) satisfy a slot's signature.
pub fn fits(slot: Slot, caps: &Capabilities) -> bool {
    let Signature {
        takes,
        gives,
        tools,
    } = slot.signature();
    let tooled = match tools {
        ToolNeed::Any => true,
        ToolNeed::Required => caps
            .text_out
            .as_ref()
            .is_some_and(|text| text.tools != ToolSupport::Absent),
    };
    caps.inputs.contains(takes) && caps.outputs.contains(gives) && tooled
}

/// The entries that can serve a slot, in catalogue order. An on-device entry is a member when one
/// of its engine profiles runs on an engine kind in `engines` and the capabilities it passes
/// through (`Capabilities::on_engine`) fit the slot; a remote entry has no engine here and is a
/// member when its capabilities fit; an attached entry is a member when its engine kind is in
/// `engines` (the wire a client speaks to it) and its capabilities fit.
pub fn slot_members<'a>(
    slot: Slot,
    catalogue: &'a [ModelEntry],
    engines: &[EngineKind],
) -> Vec<&'a ModelEntry> {
    catalogue
        .iter()
        .filter(|entry| match entry.locality {
            Locality::Remote { .. } => fits(slot, &entry.capabilities),
            Locality::OnDevice => match &entry.serving {
                Serving::Attached(attached) => {
                    engines.contains(&attached.engine) && fits(slot, &entry.capabilities)
                }
                Serving::Launched => entry
                    .engines
                    .iter()
                    .filter(|profile| engines.contains(&profile.kind))
                    .any(|profile| fits(slot, &entry.capabilities.on_engine(profile))),
            },
        })
        .collect()
}
