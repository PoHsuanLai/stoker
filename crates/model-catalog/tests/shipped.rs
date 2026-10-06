//! The shipped catalogue in the one-catalogue shape, checked against the older files it replaced.

use std::path::PathBuf;

use model_catalog::{
    CatalogKind, EngineKind, Family, Modality, ModelEntry, Slot, parse_entry, slot_members,
};

const IDS: [&str; 6] = [
    "holo-3.1-4b",
    "nemotron-3.5-asr-streaming",
    "kokoro-82m",
    "whisper-large-v3",
    "whisper-large-v3-turbo",
    "breeze-asr-25",
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
    let mut listed: Vec<String> = IDS.iter().map(|s| (*s).to_owned()).collect();
    listed.sort();
    assert_eq!(found, listed);
    for id in IDS {
        assert_eq!(shipped(id).id.0, id);
    }
}

#[test]
fn migrated_entries_keep_every_older_field() {
    for id in IDS {
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
    let catalogue: Vec<ModelEntry> = IDS.iter().map(|id| shipped(id)).collect();
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
    assert_eq!(members(Slot::Text), ["holo-3.1-4b"]);
    assert_eq!(members(Slot::ImageIn), ["holo-3.1-4b"]);
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
    for id in IDS {
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
        ("kokoro-82m", "kokoro"),
        ("whisper-large-v3", "whisper"),
        ("whisper-large-v3-turbo", "whisper"),
        ("breeze-asr-25", "whisper"),
        ("nemotron-3.5-asr-streaming", "nemotron"),
    ] {
        assert_eq!(shipped(id).family, Family(family.into()), "{id}");
    }
}
