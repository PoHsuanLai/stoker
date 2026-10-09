//! The one catalogue model: modalities, detail tables, parser rules, per-engine narrowing.

mod support;

use model_catalog::{CatalogError, DetailTable, EngineKind, Modalities, Modality, parse_entry};
use support::*;

#[test]
fn a_multimodal_entry_parses_and_derives_the_older_view() {
    let entry = parse_entry(&multimodal("mm", ENGINE)).unwrap();
    let caps = &entry.capabilities;
    assert_eq!(
        caps.inputs,
        [Modality::Text, Modality::Image, Modality::Audio].into()
    );
    assert_eq!(caps.outputs, [Modality::Text].into());
    assert!(caps.audio_in.is_some() && caps.image_in.is_some() && caps.text_out.is_some());
    assert!(caps.audio_out.is_none() && caps.vector_out.is_none() && caps.actions_out.is_none());
    // The older view: a chat model that also hears.
    use model_catalog::CatalogKind::{Llm, SpeechIn};
    assert_eq!(entry.roles, [Llm, SpeechIn].into());
    assert!(entry.caps.is_some() && entry.sampling.is_some() && entry.speech.is_some());
    assert_eq!(entry.embed, None);
}

#[test]
fn vector_and_actions_are_refused_as_inputs() {
    for bad in [Modality::Vector, Modality::Actions] {
        let slug = serde_json::to_string(&bad).unwrap();
        let text = file(
            "x",
            &format!(r#"["text", {slug}]"#),
            r#"["text"]"#,
            &[text_out(true)],
            ENGINE,
        );
        assert_eq!(
            parse_entry(&text),
            Err(CatalogError::OutputOnlyModalityAsInput { modality: bad })
        );
    }
}

#[test]
fn a_file_declares_something_in_and_something_out() {
    let none_in = file("x", "[]", r#"["text"]"#, &[text_out(false)], ENGINE);
    assert_eq!(parse_entry(&none_in), Err(CatalogError::NoInputs));
    let none_out = file("x", r#"["text"]"#, "[]", &[], ENGINE);
    assert_eq!(parse_entry(&none_out), Err(CatalogError::NoOutputs));
}

#[test]
fn a_table_without_its_modality_is_refused() {
    // Each table written for a modality the file does not declare.
    let cases: [(DetailTable, &str, &str, String); 6] = [
        (
            DetailTable::TextOut,
            r#"["text"]"#,
            r#"["vector"]"#,
            text_out(true),
        ),
        (
            DetailTable::ImageIn,
            r#"["text"]"#,
            r#"["text"]"#,
            IMAGE_IN.into(),
        ),
        (
            DetailTable::AudioIn,
            r#"["text"]"#,
            r#"["text"]"#,
            AUDIO_IN.into(),
        ),
        (
            DetailTable::AudioOut,
            r#"["text"]"#,
            r#"["text"]"#,
            AUDIO_OUT.into(),
        ),
        (
            DetailTable::VectorOut,
            r#"["text"]"#,
            r#"["text"]"#,
            VECTOR_OUT.into(),
        ),
        (
            DetailTable::ActionsOut,
            r#"["text"]"#,
            r#"["text"]"#,
            ACTIONS_OUT.into(),
        ),
    ];
    for (table, inputs, outputs, line) in cases {
        let mut tables = vec![line];
        if table != DetailTable::TextOut {
            tables.push(text_out(true));
        }
        let text = file("x", inputs, outputs, &tables, ENGINE);
        assert_eq!(
            parse_entry(&text),
            Err(CatalogError::TableWithoutModality { table }),
            "{table:?}"
        );
    }
}

#[test]
fn a_modality_without_its_table_is_refused() {
    let cases: [(DetailTable, &str, &str, Vec<String>); 6] = [
        (DetailTable::TextOut, r#"["text"]"#, r#"["text"]"#, vec![]),
        (
            DetailTable::ImageIn,
            r#"["text", "image"]"#,
            r#"["text"]"#,
            vec![text_out(true)],
        ),
        (
            DetailTable::AudioIn,
            r#"["audio"]"#,
            r#"["text"]"#,
            vec![text_out(false)],
        ),
        (DetailTable::AudioOut, r#"["text"]"#, r#"["audio"]"#, vec![]),
        (
            DetailTable::VectorOut,
            r#"["text"]"#,
            r#"["vector"]"#,
            vec![],
        ),
        (
            DetailTable::ActionsOut,
            r#"["text", "image"]"#,
            r#"["text", "actions"]"#,
            vec![text_out(true), IMAGE_IN.into()],
        ),
    ];
    for (table, inputs, outputs, tables) in cases {
        let text = file("x", inputs, outputs, &tables, ENGINE);
        assert_eq!(
            parse_entry(&text),
            Err(CatalogError::ModalityWithoutTable { table }),
            "{table:?}"
        );
    }
}

#[test]
fn a_speech_table_runs_in_its_own_direction() {
    let swapped = file(
        "x",
        r#"["audio"]"#,
        r#"["text"]"#,
        &[text_out(false), AUDIO_OUT.replace("audio_out", "audio_in")],
        ENGINE,
    );
    assert_eq!(
        parse_entry(&swapped),
        Err(CatalogError::AudioTableDirection {
            table: DetailTable::AudioIn
        })
    );
}

#[test]
fn computer_use_output_needs_text_and_image_input() {
    let no_image = file(
        "x",
        r#"["text"]"#,
        r#"["text", "actions"]"#,
        &[text_out(true), ACTIONS_OUT.into()],
        ENGINE,
    );
    assert_eq!(
        parse_entry(&no_image),
        Err(CatalogError::ActionsNeedTextAndImage)
    );
    let fine = file(
        "x",
        r#"["text", "image"]"#,
        r#"["text", "actions"]"#,
        &[text_out(true), IMAGE_IN.into(), ACTIONS_OUT.into()],
        ENGINE,
    );
    let entry = parse_entry(&fine).unwrap();
    use model_catalog::CatalogKind::{ComputerUse, Llm};
    assert_eq!(entry.roles, [Llm, ComputerUse].into());
}

#[test]
fn sampling_goes_with_text_in_and_text_out() {
    let missing = file(
        "x",
        r#"["text"]"#,
        r#"["text"]"#,
        &[text_out(false)],
        ENGINE,
    );
    assert_eq!(
        parse_entry(&missing),
        Err(CatalogError::ChatRoleWithoutSampling)
    );
    let extra = file(
        "x",
        r#"["audio"]"#,
        r#"["text"]"#,
        &[text_out(true), AUDIO_IN.into()],
        ENGINE,
    );
    assert_eq!(
        parse_entry(&extra),
        Err(CatalogError::SamplingWithoutChatRole)
    );
}

#[test]
fn the_older_keys_are_not_new_shape_keys() {
    let text = text_only("x").replace("inputs = ", "roles = [\"llm\"]\ninputs = ");
    // `roles` routes the file to the older reader, which wants its own fields.
    assert!(matches!(parse_entry(&text), Err(CatalogError::Toml(_))));
    let stray = text_only("x").replace("family = ", "speech = 1\nfamily = ");
    assert!(matches!(parse_entry(&stray), Err(CatalogError::Toml(_))));
}

#[test]
fn an_engine_narrows_what_its_model_passes_through() {
    let engines = format!(
        "{}{}",
        narrowed_engine("llama_server", r#"inputs = ["text", "image"]"#),
        engine("vllm")
    );
    let entry = parse_entry(&multimodal("mm", &engines)).unwrap();
    let caps = &entry.capabilities;
    let narrow = caps.on_engine(&entry.engines[0]);
    assert_eq!(narrow.inputs, [Modality::Text, Modality::Image].into());
    assert_eq!(narrow.outputs, caps.outputs);
    // The table of a dropped modality goes with it; the others stay.
    assert!(narrow.audio_in.is_none() && narrow.image_in.is_some() && narrow.text_out.is_some());
    // An engine that names nothing passes everything.
    assert_eq!(&caps.on_engine(&entry.engines[1]), caps);
    assert_eq!(entry.engines[0].kind, EngineKind::LlamaServer);
}

#[test]
fn narrowing_outputs_drops_their_tables() {
    let narrowed = narrowed_engine("llama_server", r#"outputs = []"#);
    let entry = parse_entry(&multimodal("mm", &narrowed)).unwrap();
    let narrow = entry.capabilities.on_engine(&entry.engines[0]);
    assert!(narrow.outputs.is_empty() && narrow.text_out.is_none());
}

#[test]
fn an_engine_may_not_pass_more_than_its_model_has() {
    let wider = narrowed_engine("llama_server", r#"inputs = ["text", "image", "audio"]"#);
    let text = text_only("t").replace(ENGINE, &wider);
    assert_eq!(
        parse_entry(&text),
        Err(CatalogError::EngineWidensModel { engine: 0 })
    );
    let wider_out = narrowed_engine("llama_server", r#"outputs = ["text", "vector"]"#);
    let text = text_only("t").replace(ENGINE, &wider_out);
    assert_eq!(
        parse_entry(&text),
        Err(CatalogError::EngineWidensModel { engine: 0 })
    );
}

#[test]
fn modalities_are_a_set_of_slugs() {
    let m: Modalities = [Modality::Audio, Modality::Text].into();
    assert_eq!(serde_json::to_string(&m).unwrap(), r#"["text","audio"]"#);
    assert!(Modality::Audio.is_input() && !Modality::Vector.is_input());
}
