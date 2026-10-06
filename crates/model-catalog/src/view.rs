//! The older view of an entry (`roles`, chat `caps`, `speech`, `embed`), derived from its
//! capabilities. porter reads these fields until it reads slots; they are never declared beside
//! the capabilities.

use std::collections::BTreeSet;

use model_provider::{
    Caps, CuaSupport, EmbedCaps, ImageCount, ImageLimits, InputKind, ModelSpace, ResizeRule,
};
use speech_provider::SpeechCaps;

use crate::{Capabilities, CatalogKind, Modality, SamplingDefaults};

/// The fields of `ModelEntry` that older readers use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyView {
    pub roles: BTreeSet<CatalogKind>,
    pub caps: Option<Caps>,
    pub sampling: Option<SamplingDefaults>,
    pub speech: Option<SpeechCaps>,
    pub embed: Option<EmbedCaps>,
}

/// What a model without an image input reports as its image limits: no images, no resizing.
const NO_IMAGES: ImageLimits = ImageLimits {
    per_prompt: ImageCount(0),
    rule: ResizeRule::Identity,
    space: ModelSpace::Image,
};

/// The kinds the capabilities make an entry eligible for. The speech view has one direction, so
/// a model with both audio tables keeps `speech_in` only; its slots say the rest.
fn roles(c: &Capabilities) -> BTreeSet<CatalogKind> {
    let takes = |m| c.inputs.contains(m);
    let gives = |m| c.outputs.contains(m);
    let speech_in = takes(Modality::Audio) && gives(Modality::Text) && c.audio_in.is_some();
    let speech_out = takes(Modality::Text)
        && gives(Modality::Audio)
        && c.audio_out.is_some()
        && c.audio_in.is_none();
    [
        (
            takes(Modality::Text) && gives(Modality::Text),
            CatalogKind::Llm,
        ),
        (
            takes(Modality::Image) && gives(Modality::Actions),
            CatalogKind::ComputerUse,
        ),
        (
            takes(Modality::Text) && gives(Modality::Vector),
            CatalogKind::Embeddings,
        ),
        (speech_in, CatalogKind::SpeechIn),
        (speech_out, CatalogKind::SpeechOut),
    ]
    .into_iter()
    .filter_map(|(member, kind)| member.then_some(kind))
    .collect()
}

fn chat_caps(c: &Capabilities) -> Option<Caps> {
    let text = c.text_out.as_ref()?;
    let inputs = [
        (Modality::Text, InputKind::Text),
        (Modality::Image, InputKind::Image),
    ]
    .into_iter()
    .filter(|(modality, _)| c.inputs.contains(*modality))
    .map(|(_, kind)| kind)
    .collect();
    Some(Caps {
        inputs,
        tools: text.tools,
        output: text.structured.clone(),
        reasoning: text.reasoning,
        streaming: text.streaming,
        images: c.image_in.unwrap_or(NO_IMAGES),
        context: text.context,
        max_output: text.max_output,
        computer_use: c.actions_out.unwrap_or(CuaSupport::Absent),
    })
}

/// The view of `capabilities`: chat caps exist when a chat role does (text out with a text or
/// image-to-actions role), the speech field is the audio table, the embed field the vector one.
pub fn derive(c: &Capabilities) -> LegacyView {
    let roles = roles(c);
    let chat = roles.contains(&CatalogKind::Llm) || roles.contains(&CatalogKind::ComputerUse);
    LegacyView {
        caps: chat.then(|| chat_caps(c)).flatten(),
        sampling: c.text_out.as_ref().and_then(|t| t.sampling),
        speech: c.audio_in.clone().or_else(|| c.audio_out.clone()),
        embed: c.vector_out.clone(),
        roles,
    }
}
