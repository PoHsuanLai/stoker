//! The shipped catalogue in the one-catalogue shape, checked against the older files it replaced.

use std::path::PathBuf;

use model_catalog::{
    CatalogKind, EngineKind, Family, Modality, ModelEntry, Slot, parse_entry, slot_members,
};

const IDS: [&str; 10] = [
    "holo-3.1-4b",
    "qwen3-4b-instruct-2507-fp8",
    "granite-4.2-3b-fp8",
    "qwen3-8b-awq",
    "qwen3.5-35b-a3b-fp8",
    "nemotron-3.5-asr-streaming",
    "kokoro-82m",
    "whisper-large-v3",
    "whisper-large-v3-turbo",
    "breeze-asr-25",
];

/// The small local text models served by vLLM (measured by hand, FINDINGS.md "Local model").
const LOCAL_TEXT: [&str; 3] = [
    "qwen3-4b-instruct-2507-fp8",
    "granite-4.2-3b-fp8",
    "qwen3-8b-awq",
];

/// The text entries served by an engine somebody else started (no `[[engine]]`, no GPU share).
const ATTACHED: [&str; 1] = ["qwen3.5-35b-a3b-fp8"];

/// The curated hosted entries, in catalogue order.
const REMOTE: [&str; 8] = [
    "claude-opus-5.5",
    "claude-haiku-4.5",
    "gemini-3.1-pro",
    "gemini-3.8-flash",
    "kimi-k3",
    "kimi-k2.6",
    "gpt-6-astra",
    "gpt-6-luna",
];

fn read(dir: &str, id: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(dir)
        .join(format!("{id}.toml"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn shipped(id: &str) -> ModelEntry {
    parse_entry(&read("../../catalog", id)).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn before(id: &str) -> ModelEntry {
    parse_entry(&read("tests/legacy", id)).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn every_shipped_file_is_listed_here_and_has_its_id_as_stem() {
    let mut found: Vec<String> =
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalog"))
            .unwrap()
            .map(|f| {
                f.unwrap()
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
    found.sort();
    let mut listed: Vec<String> = IDS.iter().chain(&REMOTE).map(|s| (*s).to_owned()).collect();
    listed.sort();
    assert_eq!(found, listed);
    for id in IDS.iter().chain(&REMOTE) {
        assert_eq!(&shipped(id).id.0, id);
    }
}

#[test]
fn migrated_entries_keep_every_older_field() {
    // The local text entries were written in the one-catalogue shape; they have no older file.
    for id in IDS
        .iter()
        .filter(|id| !LOCAL_TEXT.contains(id) && !ATTACHED.contains(id))
    {
        let (new, old) = (shipped(id), before(id));
        assert_eq!(new.roles, old.roles, "{id}: the derived kinds");
        assert_eq!(new.caps, old.caps, "{id}");
        assert_eq!(new.sampling, old.sampling, "{id}");
        assert_eq!(new.speech, old.speech, "{id}");
        assert_eq!(new.embed, old.embed, "{id}");
        assert_eq!(new.engines, old.engines, "{id}");
        assert_eq!(new.vram, old.vram, "{id}");
        assert_eq!(new.family, old.family, "{id}");
        assert_eq!(new.cold_start_estimate_s, old.cold_start_estimate_s, "{id}");
        // And an older file converts to what the migrated one declares.
        assert_eq!(
            new.capabilities, old.capabilities,
            "{id}: declared vs converted"
        );
        assert_eq!(new, old, "{id}");
    }
}

#[test]
fn the_declared_inputs_and_outputs() {
    use Modality::{Actions, Audio, Image, Text};
    for (id, inputs, outputs) in [
        ("holo-3.1-4b", vec![Text, Image], vec![Text, Actions]),
        ("nemotron-3.5-asr-streaming", vec![Audio], vec![Text]),
        ("whisper-large-v3", vec![Audio], vec![Text]),
        ("whisper-large-v3-turbo", vec![Audio], vec![Text]),
        ("breeze-asr-25", vec![Audio], vec![Text]),
        ("kokoro-82m", vec![Text], vec![Audio]),
    ] {
        let c = shipped(id).capabilities;
        assert_eq!(c.inputs.0.into_iter().collect::<Vec<_>>(), inputs, "{id}");
        assert_eq!(c.outputs.0.into_iter().collect::<Vec<_>>(), outputs, "{id}");
    }
}

#[test]
fn the_older_kinds_are_the_ones_declared_before() {
    use CatalogKind::{ComputerUse, Llm, SpeechIn, SpeechOut};
    for (id, roles) in [
        ("holo-3.1-4b", vec![Llm, ComputerUse]),
        ("nemotron-3.5-asr-streaming", vec![SpeechIn]),
        ("whisper-large-v3", vec![SpeechIn]),
        ("whisper-large-v3-turbo", vec![SpeechIn]),
        ("breeze-asr-25", vec![SpeechIn]),
        ("kokoro-82m", vec![SpeechOut]),
    ] {
        assert_eq!(
            shipped(id).roles.into_iter().collect::<Vec<_>>(),
            roles,
            "{id}"
        );
    }
}

#[test]
fn the_shipped_slots() {
    let catalogue: Vec<ModelEntry> = IDS.iter().chain(&REMOTE).map(|id| shipped(id)).collect();
    let here = [
        EngineKind::Vllm,
        EngineKind::SpeechHost,
        EngineKind::KokoroFastApi,
    ];
    let members = |slot| -> Vec<String> {
        slot_members(slot, &catalogue, &here)
            .into_iter()
            .map(|e| e.id.0.clone())
            .collect()
    };
    // The hosted entries join Text and ImageIn like any entry: each takes images and has tools.
    let hosted = REMOTE.map(String::from);
    assert_eq!(
        members(Slot::Text),
        [
            vec![
                "holo-3.1-4b".to_owned(),
                "qwen3-4b-instruct-2507-fp8".to_owned(),
                "granite-4.2-3b-fp8".to_owned(),
                "qwen3-8b-awq".to_owned(),
                "qwen3.5-35b-a3b-fp8".to_owned()
            ],
            hosted.to_vec()
        ]
        .concat()
    );
    // The two text-only local entries take no images.
    assert_eq!(
        members(Slot::ImageIn),
        [vec!["holo-3.1-4b".to_owned()], hosted.to_vec()].concat()
    );
    assert_eq!(members(Slot::ComputerUse), ["holo-3.1-4b"]);
    assert_eq!(members(Slot::VoiceOut), ["kokoro-82m"]);
    assert_eq!(members(Slot::Embeddings), Vec::<String>::new());
    assert_eq!(
        members(Slot::VoiceIn),
        [
            "nemotron-3.5-asr-streaming",
            "whisper-large-v3",
            "whisper-large-v3-turbo",
            "breeze-asr-25"
        ]
    );
}

#[test]
fn shipped_entries_round_trip_through_toml() {
    for id in IDS {
        let entry = shipped(id);
        assert_eq!(
            parse_entry(&toml::to_string(&entry).unwrap()).unwrap(),
            entry,
            "{id}"
        );
    }
}

#[test]
fn the_catalog_ranks_nothing() {
    const WORDS: &[&str] = &[
        "best",
        "recommended",
        "accurate",
        "fastest",
        "premium",
        "default tier",
    ];
    for id in IDS.iter().chain(&REMOTE) {
        let text = read("../../catalog", id).to_lowercase();
        for word in WORDS {
            assert!(!text.contains(word), "{id} says {word:?}");
        }
    }
}

#[test]
fn every_entry_names_its_family() {
    for (id, family) in [
        ("holo-3.1-4b", "qwen"),
        ("qwen3-4b-instruct-2507-fp8", "qwen"),
        ("granite-4.2-3b-fp8", "granite"),
        ("qwen3-8b-awq", "qwen"),
        ("qwen3.5-35b-a3b-fp8", "qwen"),
        ("kokoro-82m", "kokoro"),
        ("whisper-large-v3", "whisper"),
        ("whisper-large-v3-turbo", "whisper"),
        ("breeze-asr-25", "whisper"),
        ("nemotron-3.5-asr-streaming", "nemotron"),
    ] {
        assert_eq!(shipped(id).family, Family(family.into()), "{id}");
    }
}

#[test]
fn the_hosted_entries_are_remote_chat_models_with_tools_and_two_reaches() {
    use model_catalog::{ColdStartEstimateS, Locality, Modality, ProviderId, Wire};
    use model_provider::{Support, ToolSupport};
    for id in REMOTE {
        let entry = shipped(id);
        assert_eq!(entry.cold_start_estimate_s, ColdStartEstimateS(0), "{id}");
        assert!(entry.engines.is_empty(), "{id}");
        assert_eq!(
            entry.capabilities.inputs,
            [Modality::Text, Modality::Image].into(),
            "{id}"
        );
        assert_eq!(entry.capabilities.outputs, [Modality::Text].into(), "{id}");
        let text = entry.capabilities.text_out.as_ref().unwrap();
        assert_eq!(
            text.tools,
            ToolSupport::Native,
            "{id}: tool calling is required"
        );
        assert_eq!(text.reasoning, Support::Present, "{id}");
        assert!(
            text.max_output <= text.context && text.sampling.is_none(),
            "{id}"
        );
        let Locality::Remote { reach } = &entry.locality else {
            panic!("{id} is not remote");
        };
        assert_eq!(reach.len(), 2, "{id}");
        assert_eq!(reach[1].provider, ProviderId("openrouter".into()), "{id}");
        assert!(
            reach[1].model.0.contains('/'),
            "{id}: OpenRouter ids are org/name"
        );
        let direct = reach[0].provider.0.as_str();
        assert!(
            ["anthropic", "google-ai", "moonshot", "openai"].contains(&direct),
            "{id}"
        );
        let wire = if direct == "anthropic" {
            Wire::AnthropicMessages
        } else {
            Wire::OpenAiCompat
        };
        assert_eq!(reach[0].wire, wire, "{id}");
        assert_eq!(reach[1].wire, Wire::OpenAiCompat, "{id}");
        for r in reach {
            assert!(
                r.price.input_per_mtok.0 > 0
                    && r.price.output_per_mtok.0 >= r.price.input_per_mtok.0,
                "{id}"
            );
        }
        // The file says where each fact was read and when.
        let text = read("../../catalog", id);
        assert!(
            text.contains("https://") && text.contains("2026-10-06"),
            "{id}"
        );
    }
}

#[test]
fn the_hosted_entries_round_trip_and_keep_their_older_view() {
    use model_catalog::CatalogKind;
    for id in REMOTE {
        let entry = shipped(id);
        assert_eq!(
            parse_entry(&toml::to_string(&entry).unwrap()).unwrap(),
            entry,
            "{id}"
        );
        assert_eq!(entry.roles, [CatalogKind::Llm].into(), "{id}");
        assert!(entry.caps.is_some() && entry.sampling.is_none(), "{id}");
    }
}

#[test]
fn the_local_entries_stay_on_device() {
    use model_catalog::Locality;
    for id in IDS {
        assert_eq!(shipped(id).locality, Locality::OnDevice, "{id}");
    }
}

fn arg_after(entry: &ModelEntry, flag: &str) -> String {
    let args = &entry.engines[0].args;
    let at = args
        .iter()
        .position(|a| a.0 == flag)
        .unwrap_or_else(|| panic!("no {flag}"));
    args[at + 1].0.clone()
}

#[test]
fn the_local_text_entries_are_vllm_models_with_server_parsed_tools() {
    use model_provider::{Support, ToolSupport};
    for (id, parser, reasoning) in [
        ("qwen3-4b-instruct-2507-fp8", "hermes", Support::Absent),
        ("granite-4.2-3b-fp8", "qwen3_coder", Support::Present),
        ("qwen3-8b-awq", "hermes", Support::Present),
    ] {
        let entry = shipped(id);
        let text = entry.capabilities.text_out.as_ref().unwrap();
        assert_eq!(text.tools, ToolSupport::ServerParsed, "{id}");
        assert_eq!(text.reasoning, reasoning, "{id}");
        assert_eq!(entry.engines.len(), 1, "{id}");
        assert_eq!(entry.engines[0].kind, EngineKind::Vllm, "{id}");
        assert_eq!(arg_after(&entry, "--tool-call-parser"), parser, "{id}");
        assert!(
            entry.engines[0]
                .args
                .iter()
                .any(|a| a.0 == "--enable-auto-tool-choice"),
            "{id}"
        );
        // The served window is the entry's context.
        assert_eq!(
            arg_after(&entry, "--max-model-len"),
            text.context.0.to_string(),
            "{id}"
        );
    }
}

#[test]
fn the_local_text_entries_fit_the_budget_the_engine_reserves() {
    // vLLM takes `--gpu-memory-utilization` of the 16303 MiB card; the estimate is the measured
    // reservation, within 2 percent, and at most the entry's cap: 0.45 of the card (7336 MiB) for
    // the two small entries, which leaves about 6.4 GB free beside the owner's desktop, and 0.62
    // (10108 MiB) for the 8B planner, which the owner accepts to leave 3.5 GB free.
    const CARD_MIB: f64 = 16303.0;
    // Attached entries are served on another machine and have no share here: not checked.
    for (id, cap_share) in [
        ("qwen3-4b-instruct-2507-fp8", 0.45),
        ("granite-4.2-3b-fp8", 0.45),
        ("qwen3-8b-awq", 0.62),
    ] {
        let entry = shipped(id);
        let context = entry.capabilities.text_out.as_ref().unwrap().context;
        let need = f64::from(entry.vram.need(context).0);
        let share: f64 = arg_after(&entry, "--gpu-memory-utilization")
            .parse()
            .unwrap();
        let reserved = share * CARD_MIB;
        assert!(
            (need - reserved).abs() / reserved < 0.02,
            "{id}: need {need}, reserved {reserved}"
        );
        assert!(need <= cap_share * CARD_MIB + 4.0, "{id}: {need} MiB");
    }
}

#[test]
fn the_launched_entries_say_nothing_of_serving() {
    use model_catalog::Serving;
    for id in IDS.iter().filter(|id| !ATTACHED.contains(id)) {
        let entry = shipped(id);
        assert_eq!(entry.serving, Serving::Launched, "{id}");
        assert!(
            !read("../../catalog", id)
                .lines()
                .any(|l| l.starts_with("serving ")),
            "{id}: a launched entry omits the key"
        );
    }
}

#[test]
fn the_attached_35b_planner_is_a_qwen_moe_served_elsewhere() {
    use model_catalog::{
        Licence, Locality, ParserName, ServedName, Serving, Spdx, WeightSource, parse_entry,
    };
    use model_provider::{Reasoning, Support, Tokens, ToolSupport};
    let entry = shipped("qwen3.5-35b-a3b-fp8");
    assert_eq!(entry.family, Family("qwen".into()));
    assert_eq!(entry.locality, Locality::OnDevice);
    assert!(entry.engines.is_empty(), "nothing here starts it");
    let Serving::Attached(attached) = &entry.serving else {
        panic!("not attached");
    };
    assert_eq!(attached.engine, EngineKind::Vllm);
    assert_eq!(
        attached.served_name,
        ServedName("qwen3.5-35b-a3b-fp8".into())
    );
    assert_eq!(attached.tool_parser, ParserName("qwen3_coder".into()));
    assert_eq!(attached.reasoning_parser, Some(ParserName("qwen3".into())));
    assert_eq!(entry.licence, Licence::Open(Spdx("Apache-2.0".into())));
    let WeightSource::HuggingFace { repo, revision } = &entry.source else {
        panic!("not from Hugging Face");
    };
    assert_eq!(repo.0, "Qwen/Qwen3.5-35B-A3B-FP8");
    assert_eq!(revision.0, "9d1823d2dee688a6b25e77009dc727688c44936e");
    let text = entry.capabilities.text_out.as_ref().unwrap();
    assert_eq!(text.tools, ToolSupport::ServerParsed);
    assert_eq!(text.reasoning, Support::Present);
    assert_eq!(text.context, Tokens(65536));
    assert_eq!(
        text.sampling
            .unwrap()
            .for_reasoning(Reasoning::EngineDefault),
        &text.sampling.unwrap().reasoning_on,
        "proposed: the template thinks by default"
    );
    // The measured server command stays in the file for whoever hosts it.
    let file = read("../../catalog", "qwen3.5-35b-a3b-fp8");
    for flag in [
        "--max-model-len 65536",
        "--tool-call-parser qwen3_coder",
        "VLLM_USE_FLASHINFER_SAMPLER=0",
    ] {
        assert!(file.contains(flag), "{flag}");
    }
    assert_eq!(
        parse_entry(&toml::to_string(&entry).unwrap()).unwrap(),
        entry
    );
}
