//! The rules a new-shape file must keep beyond its format.

use speech_provider::SpeechDir;

use model_provider::Tokens;

use crate::{Capabilities, CatalogError, DetailTable, EngineProfile, Locality, Modality, TextOut};

/// Every table, in the order the checks run.
const TABLES: [DetailTable; 6] = [
    DetailTable::TextOut,
    DetailTable::ImageIn,
    DetailTable::AudioIn,
    DetailTable::AudioOut,
    DetailTable::VectorOut,
    DetailTable::ActionsOut,
];

/// A model takes and gives something; `vector` and `actions` are never inputs; a detail table is
/// written exactly when its modality is declared; the speech tables run in their own direction;
/// computer-use output comes with the text and image sides it is driven through; sampling is
/// written exactly when text is also an input; the context holds the output; where stated, the
/// served window and reply stay within the model's own maxima and its minimum stays within its
/// maximum (the served window may sit below the minimum: an older line that serves less than the
/// model is useful at is data, not an error).
pub fn capabilities(c: &Capabilities, locality: &Locality) -> Result<(), CatalogError> {
    if c.inputs.is_empty() {
        return Err(CatalogError::NoInputs);
    }
    if c.outputs.is_empty() {
        return Err(CatalogError::NoOutputs);
    }
    if let Some(modality) = c.inputs.first_output_only() {
        return Err(CatalogError::OutputOnlyModalityAsInput { modality });
    }
    for table in TABLES {
        match (c.declares(table), c.has_table(table)) {
            (false, true) => return Err(CatalogError::TableWithoutModality { table }),
            (true, false) => return Err(CatalogError::ModalityWithoutTable { table }),
            _ => {}
        }
    }
    speech_directions(c)?;
    chat(c, locality)
}

fn speech_directions(c: &Capabilities) -> Result<(), CatalogError> {
    let wrong = [
        (&c.audio_in, SpeechDir::In, DetailTable::AudioIn),
        (&c.audio_out, SpeechDir::Out, DetailTable::AudioOut),
    ]
    .into_iter()
    .find(|(caps, dir, _)| caps.as_ref().is_some_and(|caps| caps.dir() != *dir));
    match wrong {
        Some((_, _, table)) => Err(CatalogError::AudioTableDirection { table }),
        None => Ok(()),
    }
}

fn chat(c: &Capabilities, locality: &Locality) -> Result<(), CatalogError> {
    let text_in = c.inputs.contains(Modality::Text);
    if c.actions_out.is_some()
        && !(text_in && c.inputs.contains(Modality::Image) && c.text_out.is_some())
    {
        return Err(CatalogError::ActionsNeedTextAndImage);
    }
    let Some(text) = &c.text_out else {
        return Ok(());
    };
    if text.max_output > text.context {
        return Err(CatalogError::OutputExceedsContext);
    }
    limits(text)?;
    match (text_in, &text.sampling) {
        // A hosted model takes the provider's defaults: nothing to write.
        (true, None) if !locality.is_on_device() => Ok(()),
        (true, None) => Err(CatalogError::ChatRoleWithoutSampling),
        (false, Some(_)) => Err(CatalogError::SamplingWithoutChatRole),
        _ => Ok(()),
    }
}

fn limits(text: &TextOut) -> Result<(), CatalogError> {
    let past = |value: Tokens, bound: Option<Tokens>| bound.is_some_and(|b| value > b);
    if past(text.context, text.context_max) {
        return Err(CatalogError::ContextExceedsModelMax);
    }
    if past(text.max_output, text.max_output_max) {
        return Err(CatalogError::OutputExceedsModelMax);
    }
    match text.context_min {
        Some(min) if past(min, text.context_max) => Err(CatalogError::MinimumExceedsModelMax),
        _ => Ok(()),
    }
}

/// An engine may pass fewer inputs and outputs than its model has, never more.
pub fn narrowing(c: &Capabilities, engines: &[EngineProfile]) -> Result<(), CatalogError> {
    let widens = |passed: &Option<crate::Modalities>, declared: &crate::Modalities| {
        passed
            .as_ref()
            .is_some_and(|set| !set.is_subset_of(declared))
    };
    match engines
        .iter()
        .position(|e| widens(&e.inputs, &c.inputs) || widens(&e.outputs, &c.outputs))
    {
        Some(engine) => Err(CatalogError::EngineWidensModel { engine }),
        None => Ok(()),
    }
}
