//! Slots: the jobs a person picks a model for, each a capability signature.
//!
//! A model is listed in every slot its capabilities satisfy, so one entry may be in several. The
//! rules are pure functions of the catalogue; nothing is read from a model's name.
//!
//! Later slots: `image_gen` (text in, image out) and `rerank` stay out until an entry needs them.

use serde::{Deserialize, Serialize};

use crate::{Capabilities, EngineKind, Modality, ModelEntry};

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

/// What a slot needs: one modality taken in and one given out. A model may take and give more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature {
    pub takes: Modality,
    pub gives: Modality,
}

impl Slot {
    /// The slot's capability signature. Tool support is not part of it: a text model without
    /// tools still serves the slot, and a caller that needs tools reads `text_out`.
    pub fn signature(self) -> Signature {
        let (takes, gives) = match self {
            Slot::Text => (Modality::Text, Modality::Text),
            Slot::VoiceIn => (Modality::Audio, Modality::Text),
            Slot::VoiceOut => (Modality::Text, Modality::Audio),
            Slot::ImageIn => (Modality::Image, Modality::Text),
            Slot::ComputerUse => (Modality::Image, Modality::Actions),
            Slot::Embeddings => (Modality::Text, Modality::Vector),
        };
        Signature { takes, gives }
    }
}

/// Whether capabilities (usually engine-narrowed) satisfy a slot's signature.
pub fn fits(slot: Slot, caps: &Capabilities) -> bool {
    let Signature { takes, gives } = slot.signature();
    caps.inputs.contains(takes) && caps.outputs.contains(gives)
}

/// The entries that can serve a slot on this computer, in catalogue order: an entry is a member
/// when one of its engine profiles runs on an engine kind in `engines` and the capabilities it
/// passes through (`Capabilities::on_engine`) fit the slot.
pub fn slot_members<'a>(
    slot: Slot,
    catalogue: &'a [ModelEntry],
    engines: &[EngineKind],
) -> Vec<&'a ModelEntry> {
    catalogue
        .iter()
        .filter(|entry| {
            entry
                .engines
                .iter()
                .filter(|profile| engines.contains(&profile.kind))
                .any(|profile| fits(slot, &entry.capabilities.on_engine(profile)))
        })
        .collect()
}
