use std::path::PathBuf;

use cua_action::{CuaDialect, GridMax, ModelSpace, ToolDialect};
use model_catalog::{
    CatalogError, CatalogId, CatalogKind, ColdStartEstimateS, EngineKind, Family, GpuNeed, MiB,
    ModelEntry, VramEstimate, WeightFiles, merge_catalogs, parse_entry,
};
use model_provider::{CuaSupport, Support, Tokens};
use speech_provider::{
    AudioFormat, AudioMs, Lang, LangSet, PcmFormat, SampleRate, SpeechDir, SpeechIo,
};
use vision_prep::{PatchFactor, PixelCount, ResizeRule};

fn catalog_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog")
}

fn holo_text() -> String {
    std::fs::read_to_string(catalog_dir().join("holo-3.1-4b.toml")).unwrap()
}

#[test]
fn shipped_files_parse() {
    let mut parsed = 0;
    for file in std::fs::read_dir(catalog_dir()).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let entry = parse_entry(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            path.file_stem().unwrap().to_str().unwrap(),
            entry.id.0,
            "the id is the file's stem"
        );
        parsed += 1;
    }
    assert_eq!(parsed, 6);
}

#[test]
fn holo_entry_says_what_s0_found() {
    let entry = parse_entry(&holo_text()).unwrap();
    let caps = entry.caps.as_ref().unwrap();
    assert_eq!(entry.speech, None);
    assert_eq!(
        caps.images.rule,
        ResizeRule::SmartResize {
            factor: PatchFactor(32),
            min_pixels: PixelCount(65_536),
            max_pixels: PixelCount(16_777_216),
        }
    );
    assert_eq!(caps.images.space, ModelSpace::Grid(GridMax(1000)));
    assert!(matches!(
        caps.computer_use,
        CuaSupport::Dialect {
            dialect: CuaDialect::Tool(ToolDialect::Holo31),
            ..
        }
    ));
    assert_eq!(
        entry.roles,
        [CatalogKind::Llm, CatalogKind::ComputerUse].into()
    );
    assert_eq!(entry.engines.len(), 1);
    assert_eq!(entry.engines[0].kind, EngineKind::Vllm);
}

#[test]
fn entry_round_trips_through_toml() {
    let entry = parse_entry(&holo_text()).unwrap();
    let text = toml::to_string(&entry).unwrap();
    assert_eq!(parse_entry(&text).unwrap(), entry);
}

#[test]
fn missing_field_refuses() {
    let text = holo_text();
    for field in [
        "label",
        "licence",
        "vram",
        "context",
        "images",
        "computer_use",
        "roles",
        "tools",
    ] {
        let without: String = text
            .lines()
            .filter(|l| !l.starts_with(&format!("{field} ")))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            matches!(parse_entry(&without), Err(CatalogError::Toml(_))),
            "a file without {field} must be refused"
        );
    }
}

#[test]
fn a_chat_entry_writes_its_sampling_defaults() {
    use model_catalog::ReasoningDefault;
    use model_provider::{Count, Effort, Knob, Milli, Reasoning};
    let entry = parse_entry(&holo_text()).unwrap();
    let sampling = entry.sampling.unwrap();
    assert_eq!(sampling.reasoning_on.temperature, Milli(600));
    assert_eq!(sampling.reasoning_on.top_k, Knob::Set(Count(20)));
    assert_eq!(sampling.reasoning_off.temperature, Milli(0));
    assert_eq!(sampling.reasoning_off.top_p, Knob::Off);
    assert_eq!(sampling.reasoning_default, ReasoningDefault::On);
    // Nothing said about reasoning takes the sampling of what the model does by itself.
    assert_eq!(
        sampling.for_reasoning(Reasoning::EngineDefault),
        &sampling.reasoning_on
    );
    assert_eq!(
        sampling.for_reasoning(Reasoning::Off),
        &sampling.reasoning_off
    );
    assert_eq!(
        sampling.for_reasoning(Reasoning::On(Effort::Low)),
        &sampling.reasoning_on
    );

    let without: String = holo_text()
        .lines()
        .filter(|l| !l.starts_with("sampling "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parse_entry(&without),
        Err(CatalogError::ChatRoleWithoutSampling)
    );

    let undecided = holo_text().replace(", reasoning_default = \"on\"", "");
    assert!(matches!(
        parse_entry(&undecided),
        Err(CatalogError::Toml(_))
    ));
}

#[test]
fn a_speech_entry_may_not_write_sampling() {
    let text = std::fs::read_to_string(catalog_dir().join("kokoro-82m.toml")).unwrap();
    let with = format!(
        "{}\nsampling = {{ reasoning_on = {{ temperature = 0, top_p = {{ kind = \"off\" }}, top_k = {{ kind = \"off\" }}, min_p = {{ kind = \"off\" }}, repeat_penalty = {{ kind = \"off\" }}, seed = {{ kind = \"off\" }} }}, reasoning_off = {{ temperature = 0, top_p = {{ kind = \"off\" }}, top_k = {{ kind = \"off\" }}, min_p = {{ kind = \"off\" }}, repeat_penalty = {{ kind = \"off\" }}, seed = {{ kind = \"off\" }} }}, reasoning_default = \"off\" }}",
        text.split("[[engine]]").next().unwrap().trim_end()
    ) + "\n[[engine]]"
        + text.split("[[engine]]").nth(1).unwrap();
    assert_eq!(
        parse_entry(&with),
        Err(CatalogError::SamplingWithoutChatRole)
    );
}

#[test]
fn the_new_constraints_parse_in_the_output_list() {
    let text = holo_text().replace(
        r#"output = ["json_schema"]"#,
        r#"output = ["json_schema", "regex", "lark", "gbnf", "choice"]"#,
    );
    let entry = parse_entry(&text).unwrap();
    assert_eq!(entry.caps.unwrap().output.len(), 5);
}

#[test]
fn an_entry_without_an_engine_is_refused() {
    let text = holo_text();
    let head = text.split("[[engine]]").next().unwrap();
    // With no `[[engine]]` table the field is missing, which serde reports before the check.
    assert!(matches!(parse_entry(head), Err(CatalogError::Toml(_))));
}

#[test]
fn user_file_replaces_system() {
    let system = parse_entry(&holo_text()).unwrap();
    let mut user = system.clone();
    user.label = "My Holo".into();
    let mut extra = system.clone();
    extra.id = CatalogId("mine".into());
    let merged = merge_catalogs(vec![system.clone()], vec![user.clone(), extra.clone()]);
    assert_eq!(merged, vec![user, extra]);
}

#[test]
fn vram_need_adds_weights_kv_and_overhead() {
    let vram = VramEstimate {
        weights: MiB(10_400),
        kv_per_1k_ctx: MiB(32),
        overhead: MiB(1500),
    };
    assert_eq!(vram.need(Tokens(32_768)), MiB(10_400 + 1049 + 1500)); // 32 * 32.768 = 1048.6 rounds up
    assert_eq!(vram.need(Tokens(0)), MiB(11_900));
}

fn shipped(id: &str) -> ModelEntry {
    let text = std::fs::read_to_string(catalog_dir().join(format!("{id}.toml"))).unwrap();
    parse_entry(&text).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn lang(tag: &str) -> Lang {
    Lang::new(tag).unwrap()
}

#[test]
fn speech_entries_parse() {
    let nemotron = shipped("nemotron-3.5-asr-streaming");
    assert_eq!(nemotron.roles, [CatalogKind::SpeechIn].into());
    assert_eq!(nemotron.caps, None);
    let caps = nemotron.speech.as_ref().unwrap();
    assert_eq!(caps.dir(), SpeechDir::In);
    assert_eq!(caps.streaming, Support::Present);
    assert_eq!(caps.partials, Support::Present);
    assert_eq!(caps.max_audio, AudioMs(120_000));
    let LangSet::Listed(langs) = &caps.langs else {
        panic!("Nemotron lists its languages");
    };
    assert_eq!(langs.len(), 40, "the model card's 40 locales");
    assert!(langs.contains(&lang("zh-CN")) && langs.contains(&lang("en-US")));
    assert_eq!(
        caps.io,
        SpeechIo::In {
            input: AudioFormat {
                rate: SampleRate(16_000),
                pcm: PcmFormat::S16Le
            }
        }
    );
    assert_eq!(nemotron.engines[0].kind, EngineKind::SpeechHost);
    assert_eq!(nemotron.engines[0].weights, WeightFiles::SherpaDir);

    let kokoro = shipped("kokoro-82m");
    assert_eq!(kokoro.roles, [CatalogKind::SpeechOut].into());
    let caps = kokoro.speech.as_ref().unwrap();
    assert_eq!(caps.dir(), SpeechDir::Out);
    let SpeechIo::Out { output, voices } = &caps.io else {
        panic!("Kokoro is a voice");
    };
    assert_eq!(output.rate, SampleRate(24_000));
    assert_eq!(voices.len(), 3);
    assert_eq!(kokoro.engines[0].kind, EngineKind::KokoroFastApi);

    for id in [
        "whisper-large-v3-turbo",
        "whisper-large-v3",
        "breeze-asr-25",
    ] {
        let entry = shipped(id);
        assert_eq!(entry.roles, [CatalogKind::SpeechIn].into(), "{id}");
        let caps = entry.speech.as_ref().unwrap();
        assert_eq!(caps.streaming, Support::Absent, "{id}");
        assert_eq!(entry.engines[0].kind, EngineKind::Vllm, "{id}");
    }
    assert_eq!(
        shipped("breeze-asr-25").speech.unwrap().langs,
        LangSet::Listed([lang("en"), lang("zh-TW")].into())
    );
    assert_eq!(
        shipped("whisper-large-v3").speech.unwrap().langs,
        LangSet::Any
    );
}

#[test]
fn speech_entries_round_trip_through_toml() {
    for id in [
        "nemotron-3.5-asr-streaming",
        "kokoro-82m",
        "whisper-large-v3-turbo",
        "whisper-large-v3",
        "breeze-asr-25",
    ] {
        let entry = shipped(id);
        let text = toml::to_string(&entry).unwrap();
        assert_eq!(parse_entry(&text).unwrap(), entry, "{id}");
    }
}

#[test]
fn speech_role_requires_speech_table() {
    let text = std::fs::read_to_string(catalog_dir().join("kokoro-82m.toml")).unwrap();
    let without_table: String = text
        .lines()
        .filter(|l| !l.starts_with("speech "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parse_entry(&without_table),
        Err(CatalogError::SpeechRoleWithoutSpeechTable)
    );
    let wrong_role = text.replace(r#"roles = ["speech_out"]"#, r#"roles = ["speech_in"]"#);
    assert_eq!(
        parse_entry(&wrong_role),
        Err(CatalogError::SpeechDirectionMismatch {
            role: CatalogKind::SpeechIn
        })
    );
    let both = text.replace(
        r#"roles = ["speech_out"]"#,
        r#"roles = ["speech_in", "speech_out"]"#,
    );
    assert_eq!(parse_entry(&both), Err(CatalogError::BothSpeechDirections));
    let none = text.replace(r#"roles = ["speech_out"]"#, "roles = []");
    assert_eq!(parse_entry(&none), Err(CatalogError::NoRoles));
    // A chat entry may not carry a speech table.
    let holo = format!(
        "{}\nspeech = {{ dir = \"in\", streaming = \"absent\", partials = \"absent\", punctuation = \"absent\", timestamps = \"absent\", langs = {{ kind = \"any\" }}, max_audio_ms = 1000, input = {{ rate = 16000, pcm = \"s16_le\" }} }}\n",
        holo_text().split("[[engine]]").next().unwrap()
    );
    let holo = format!(
        "{holo}[[engine]]\nkind = \"vllm\"\nargs = []\nweights = {{ kind = \"hf_snapshot\" }}\n"
    );
    assert_eq!(
        parse_entry(&holo),
        Err(CatalogError::SpeechTableWithoutSpeechRole)
    );
}

#[test]
fn a_speech_entry_does_not_carry_chat_fields() {
    let text = std::fs::read_to_string(catalog_dir().join("kokoro-82m.toml")).unwrap();
    let holo = holo_text();
    let chat_lines: String = holo
        .lines()
        .filter(|l| {
            [
                "inputs ",
                "tools ",
                "output ",
                "reasoning ",
                "streaming ",
                "context ",
                "max_output ",
                "images ",
                "computer_use ",
            ]
            .iter()
            .any(|f| l.starts_with(f))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let head = text.split("[[engine]]").next().unwrap();
    let tail = text.split("[[engine]]").nth(1).unwrap();
    let mixed = format!("{head}\n{chat_lines}\n[[engine]]{tail}");
    assert_eq!(
        parse_entry(&mixed),
        Err(CatalogError::ChatFieldsWithoutChatRole)
    );
}

#[test]
fn zero_vram_means_no_gpu() {
    for (id, need) in [
        ("nemotron-3.5-asr-streaming", GpuNeed::Absent),
        ("kokoro-82m", GpuNeed::Absent),
        ("whisper-large-v3-turbo", GpuNeed::Needed),
        ("whisper-large-v3", GpuNeed::Needed),
        ("breeze-asr-25", GpuNeed::Needed),
        ("holo-3.1-4b", GpuNeed::Needed),
    ] {
        assert_eq!(shipped(id).vram.gpu_need(), need, "{id}");
    }
    // A vLLM engine with an all-zero estimate is a mistake in the file.
    let text = std::fs::read_to_string(catalog_dir().join("whisper-large-v3.toml")).unwrap();
    let zeroed = text.replace(
        "vram = { weights_mib = 2960, kv_per_1k_ctx_mib = 0, overhead_mib = 1500 }",
        "vram = { weights_mib = 0, kv_per_1k_ctx_mib = 0, overhead_mib = 0 }",
    );
    assert_eq!(parse_entry(&zeroed), Err(CatalogError::VllmWithoutVram));
}

#[test]
fn the_catalog_ranks_nothing() {
    // The picker is a plain list: no file says which model is better than another.
    const WORDS: &[&str] = &[
        "best",
        "recommended",
        "accurate",
        "fastest",
        "premium",
        "default tier",
    ];
    for file in std::fs::read_dir(catalog_dir()).unwrap() {
        let path = file.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
        for word in WORDS {
            assert!(!text.contains(word), "{} says {word:?}", path.display());
        }
    }
}

#[test]
fn speech_slugs_are_stable() {
    assert_eq!(
        serde_json::to_string(&CatalogKind::SpeechIn).unwrap(),
        r#""speech_in""#
    );
    assert_eq!(
        serde_json::to_string(&CatalogKind::SpeechOut).unwrap(),
        r#""speech_out""#
    );
    assert_eq!(
        serde_json::to_string(&EngineKind::SpeechHost).unwrap(),
        r#""speech_host""#
    );
    assert_eq!(
        serde_json::to_string(&EngineKind::KokoroFastApi).unwrap(),
        r#""kokoro_fast_api""#
    );
    assert_eq!(
        serde_json::to_string(&WeightFiles::SherpaDir).unwrap(),
        r#"{"kind":"sherpa_dir"}"#
    );
}

#[test]
fn every_entry_names_its_family_and_a_cold_start_estimate() {
    for (id, family, secs) in [
        ("holo-3.1-4b", "qwen", 180),
        ("kokoro-82m", "kokoro", 10),
        ("whisper-large-v3", "whisper", 90),
        ("whisper-large-v3-turbo", "whisper", 60),
        ("breeze-asr-25", "whisper", 90),
        ("nemotron-3.5-asr-streaming", "nemotron", 5),
    ] {
        let entry = shipped(id);
        assert_eq!(entry.family, Family(family.into()), "{id}");
        assert_eq!(
            entry.cold_start_estimate_s,
            ColdStartEstimateS(secs),
            "{id}"
        );
        assert_eq!(entry.family.0, entry.family.0.to_lowercase(), "{id}");
    }
}

#[test]
fn a_file_without_family_or_estimate_is_refused() {
    for line in ["family = \"qwen\"\n", "cold_start_estimate_s = 180\n"] {
        let text = holo_text();
        assert!(text.contains(line));
        let Err(CatalogError::Toml(msg)) = parse_entry(&text.replace(line, "")) else {
            panic!("a missing field must be refused");
        };
        let field = line.split(' ').next().unwrap();
        assert!(msg.contains(field), "{msg}");
    }
}
