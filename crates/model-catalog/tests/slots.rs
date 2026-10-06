//! Slots: signatures as data, `fits`, and `slot_members`.

mod support;

use model_catalog::{
    EngineKind, Modality, ModelEntry, Signature, Slot, ToolNeed, fits, parse_entry, slot_members,
};
use support::*;

fn entry(text: &str) -> ModelEntry {
    parse_entry(text).unwrap()
}

fn ids(members: Vec<&ModelEntry>) -> Vec<&str> {
    members.into_iter().map(|e| e.id.0.as_str()).collect()
}

/// One of each shape, in this order.
fn catalogue() -> Vec<ModelEntry> {
    let llama = engine("llama_server");
    vec![
        entry(&multimodal("hears-sees-reads", &llama)),
        entry(&text_only("reads")),
        entry(&file(
            "transcribes",
            r#"["audio"]"#,
            r#"["text"]"#,
            &[text_out(false), AUDIO_IN.into()],
            &llama,
        )),
        entry(&file(
            "speaks",
            r#"["text"]"#,
            r#"["audio"]"#,
            &[AUDIO_OUT.into()],
            &llama,
        )),
        entry(&file(
            "embeds",
            r#"["text"]"#,
            r#"["vector"]"#,
            &[VECTOR_OUT.into()],
            &llama,
        )),
        entry(&file(
            "drives",
            r#"["text", "image"]"#,
            r#"["text", "actions"]"#,
            &[text_out(true), IMAGE_IN.into(), ACTIONS_OUT.into()],
            &llama,
        )),
    ]
}

#[test]
fn each_slot_names_its_signature() {
    let sig = |takes, gives| Signature {
        takes,
        gives,
        tools: if (takes, gives) == (Modality::Text, Modality::Text) {
            ToolNeed::Required
        } else {
            ToolNeed::Any
        },
    };
    assert_eq!(Slot::Text.signature(), sig(Modality::Text, Modality::Text));
    assert_eq!(
        Slot::VoiceIn.signature(),
        sig(Modality::Audio, Modality::Text)
    );
    assert_eq!(
        Slot::VoiceOut.signature(),
        sig(Modality::Text, Modality::Audio)
    );
    assert_eq!(
        Slot::ImageIn.signature(),
        sig(Modality::Image, Modality::Text)
    );
    assert_eq!(
        Slot::ComputerUse.signature(),
        sig(Modality::Image, Modality::Actions)
    );
    assert_eq!(
        Slot::Embeddings.signature(),
        sig(Modality::Text, Modality::Vector)
    );
}

#[test]
fn slot_slugs_are_the_settings_segments() {
    for (slot, slug) in [
        (Slot::Text, "text"),
        (Slot::VoiceIn, "voice_in"),
        (Slot::VoiceOut, "voice_out"),
        (Slot::ImageIn, "image_in"),
        (Slot::ComputerUse, "computer_use"),
        (Slot::Embeddings, "embeddings"),
    ] {
        assert_eq!(serde_json::to_string(&slot).unwrap(), format!("\"{slug}\""));
    }
}

#[test]
fn one_model_fits_every_slot_its_capabilities_satisfy() {
    let model = entry(&multimodal("mm", &engine("llama_server")));
    let caps = &model.capabilities;
    assert!(fits(Slot::Text, caps));
    assert!(fits(Slot::ImageIn, caps));
    assert!(fits(Slot::VoiceIn, caps));
    assert!(!fits(Slot::VoiceOut, caps));
    assert!(!fits(Slot::ComputerUse, caps));
    assert!(!fits(Slot::Embeddings, caps));
}

#[test]
fn members_come_in_catalogue_order() {
    let catalogue = catalogue();
    let all = [EngineKind::LlamaServer];
    let members = |slot| ids(slot_members(slot, &catalogue, &all));
    assert_eq!(members(Slot::Text), ["hears-sees-reads", "reads", "drives"]);
    assert_eq!(members(Slot::VoiceIn), ["hears-sees-reads", "transcribes"]);
    assert_eq!(members(Slot::VoiceOut), ["speaks"]);
    assert_eq!(members(Slot::ImageIn), ["hears-sees-reads", "drives"]);
    assert_eq!(members(Slot::ComputerUse), ["drives"]);
    assert_eq!(members(Slot::Embeddings), ["embeds"]);
}

#[test]
fn an_engine_that_is_not_here_gives_no_members() {
    let catalogue = catalogue();
    assert!(slot_members(Slot::Text, &catalogue, &[EngineKind::Vllm]).is_empty());
    assert!(slot_members(Slot::Text, &catalogue, &[]).is_empty());
}

#[test]
fn a_narrowed_engine_offers_only_what_it_passes() {
    // Hears on vLLM; the llama-server build has no audio path.
    let engines = format!(
        "{}{}",
        narrowed_engine("llama_server", r#"inputs = ["text", "image"]"#),
        engine("vllm")
    );
    let catalogue = vec![entry(&multimodal("mm", &engines))];
    let llama = [EngineKind::LlamaServer];
    assert_eq!(ids(slot_members(Slot::Text, &catalogue, &llama)), ["mm"]);
    assert_eq!(ids(slot_members(Slot::ImageIn, &catalogue, &llama)), ["mm"]);
    assert!(slot_members(Slot::VoiceIn, &catalogue, &llama).is_empty());
    let both = [EngineKind::LlamaServer, EngineKind::Vllm];
    assert_eq!(ids(slot_members(Slot::VoiceIn, &catalogue, &both)), ["mm"]);
    // Only the narrowed engine is on this computer: audio is not offered even though the model
    // declares it.
    assert!(fits(Slot::VoiceIn, &catalogue[0].capabilities));
}

#[test]
fn the_text_slot_needs_tools_and_the_others_do_not() {
    let model = |tools: &str| {
        entry(&text_only("t").replace(r#"tools = "native""#, &format!("tools = \"{tools}\"")))
    };
    let catalogue = vec![model("native"), model("server_parsed"), model("absent")];
    let llama = [EngineKind::LlamaServer];
    assert_eq!(slot_members(Slot::Text, &catalogue, &llama).len(), 2);
    // Without tools it still reads images, if it takes them: that slot asks for no tools.
    assert!(fits(
        Slot::Embeddings,
        &entry(&file(
            "e",
            r#"["text"]"#,
            r#"["vector"]"#,
            &[VECTOR_OUT.into()],
            ENGINE
        ))
        .capabilities
    ));
}
