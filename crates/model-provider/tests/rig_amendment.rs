//! The types the rig amendment added to the frozen interface: round trips, pinned JSON and the
//! tables that are pure data.

use model_provider::{
    Attempt, BatchMax, CharCount, ChoiceText, Constraint, Count, Dims, EmbedCaps, EmbedEnd,
    EmbedFault, EmbedPrompts, EmbedRole, EmbedTurn, EmbedVector, EngineExtras, Field, FieldName,
    KeepAlive, Knob, LlamaExtras, Milli, ModelName, OllamaExtras, OpaqueText, OutputShape, Part,
    Permille, PrefixText, PromptCache, ProviderError, Reasoning, RetryClass, RetryPolicy,
    RetrySeconds, Sampling, SchemaDialect, Seconds, Seed, ServerStatus, Shape, ShapeKind,
    ShapeWithTools, SignatureText, SlotId, StopReason, ThoughtSeal, Tokens, ToolParallelism,
    TurnUsage, Variant, VariantName, VllmExtras, WaitMs,
};

fn round_trip<T>(value: &T, json: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + core::fmt::Debug,
{
    assert_eq!(serde_json::to_string(value).unwrap(), json);
    assert_eq!(&serde_json::from_str::<T>(json).unwrap(), value);
}

#[test]
fn knob_is_written_in_full() {
    round_trip(&Knob::<Milli>::Off, r#"{"kind":"off"}"#);
    round_trip(&Knob::Set(Milli(950)), r#"{"kind":"set","v":950}"#);
}

#[test]
fn sampling_round_trips_with_pinned_json() {
    let sampling = Sampling {
        temperature: Milli(600),
        top_p: Knob::Set(Milli(950)),
        top_k: Knob::Set(Count(20)),
        min_p: Knob::Off,
        repeat_penalty: Knob::Set(Milli(1100)),
        seed: Knob::Set(Seed(7)),
    };
    round_trip(
        &sampling,
        r#"{"temperature":600,"top_p":{"kind":"set","v":950},"top_k":{"kind":"set","v":20},"min_p":{"kind":"off"},"repeat_penalty":{"kind":"set","v":1100},"seed":{"kind":"set","v":7}}"#,
    );
}

#[test]
fn engine_extras_have_one_arm_per_flavor() {
    let cases: Vec<(EngineExtras, &str)> = vec![
        (EngineExtras::None, r#"{"kind":"none"}"#),
        (
            EngineExtras::LlamaServer(LlamaExtras {
                cache_prompt: PromptCache::Reuse,
                slot: Knob::Set(SlotId(2)),
            }),
            r#"{"kind":"llama_server","v":{"cache_prompt":"reuse","slot":{"kind":"set","v":2}}}"#,
        ),
        (
            EngineExtras::Vllm(VllmExtras {
                priority: Knob::Set(Count(3)),
            }),
            r#"{"kind":"vllm","v":{"priority":{"kind":"set","v":3}}}"#,
        ),
        (
            EngineExtras::Ollama(OllamaExtras {
                keep_alive: KeepAlive::For(Seconds(300)),
                num_ctx: Knob::Set(Tokens(8192)),
            }),
            r#"{"kind":"ollama","v":{"keep_alive":{"kind":"for","v":300},"num_ctx":{"kind":"set","v":8192}}}"#,
        ),
    ];
    for (extras, json) in &cases {
        round_trip(extras, json);
    }
    for keep in [
        KeepAlive::EngineDefault,
        KeepAlive::Unload,
        KeepAlive::Forever,
    ] {
        let json = serde_json::to_string(&keep).unwrap();
        assert_eq!(serde_json::from_str::<KeepAlive>(&json).unwrap(), keep);
    }
}

#[test]
fn tool_parallelism_and_prompt_cache_slugs() {
    round_trip(&ToolParallelism::One, r#""one""#);
    round_trip(&ToolParallelism::Many, r#""many""#);
    round_trip(&PromptCache::Fresh, r#""fresh""#);
    round_trip(&ShapeWithTools::AfterResult, r#""after_result""#);
}

#[test]
fn a_thought_carries_its_seal() {
    let signed = Part::Thought {
        text: "hm".into(),
        seal: ThoughtSeal::Signed(SignatureText("sig".into())),
    };
    round_trip(
        &signed,
        r#"{"kind":"thought","v":{"text":"hm","seal":{"kind":"signed","v":"sig"}}}"#,
    );
    let redacted = Part::Thought {
        text: String::new(),
        seal: ThoughtSeal::Redacted(OpaqueText("enc".into())),
    };
    round_trip(
        &redacted,
        r#"{"kind":"thought","v":{"text":"","seal":{"kind":"redacted","v":"enc"}}}"#,
    );
    round_trip(&ThoughtSeal::None, r#"{"kind":"none"}"#);
    assert_eq!(
        format!("{:?}", OpaqueText("enc".into())),
        "OpaqueText(<3 bytes>)"
    );
}

#[test]
fn the_new_output_shapes_and_constraints_round_trip() {
    round_trip(
        &OutputShape::Gbnf("root ::= \"y\" | \"n\"".into()),
        r#"{"kind":"gbnf","v":"root ::= \"y\" | \"n\""}"#,
    );
    round_trip(
        &OutputShape::Choice(vec!["allow".into(), "deny".into()]),
        r#"{"kind":"choice","v":["allow","deny"]}"#,
    );
    round_trip(&Constraint::Gbnf, r#""gbnf""#);
    round_trip(&Constraint::Choice, r#""choice""#);
}

#[test]
fn a_server_error_carries_only_its_status() {
    round_trip(
        &ProviderError::Server(ServerStatus(503)),
        r#"{"kind":"server","v":503}"#,
    );
}

#[test]
fn usage_counts_cached_tokens() {
    let usage = TurnUsage {
        cached: Tokens(8),
        ..TurnUsage::default()
    };
    round_trip(&usage, r#"{"input":0,"output":0,"cached":8,"images":0}"#);
}

#[test]
fn stop_reasons_map_to_the_convention_finish_reasons() {
    use genai_names::Finish;
    const CASES: &[(StopReason, Finish)] = &[
        (StopReason::EndTurn, Finish::Stop),
        (StopReason::StopSequence, Finish::Stop),
        (StopReason::ToolUse, Finish::ToolCalls),
        (StopReason::MaxTokens, Finish::Length),
        (StopReason::ContentFilter, Finish::ContentFilter),
    ];
    for (stop, finish) in CASES {
        assert_eq!(Finish::from(*stop), *finish);
    }
}

#[test]
fn embedding_types_round_trip_and_the_inputs_do_not_print() {
    let turn = EmbedTurn {
        model: ModelName("nomic-embed-text".into()),
        inputs: vec!["a private note".into()],
        role: EmbedRole::Document,
        dims: Knob::Set(Dims(768)),
    };
    round_trip(
        &turn,
        r#"{"model":"nomic-embed-text","inputs":["a private note"],"role":"document","dims":{"kind":"set","v":768}}"#,
    );
    assert!(!format!("{turn:?}").contains("private"));
    let end = EmbedEnd {
        vectors: vec![EmbedVector(vec![0.5, -0.25])],
        usage: TurnUsage::default(),
        served: ModelName("nomic-embed-text".into()),
    };
    let json = serde_json::to_string(&end).unwrap();
    assert_eq!(serde_json::from_str::<EmbedEnd>(&json).unwrap(), end);
    round_trip(&EmbedRole::Query, r#""query""#);
}

#[test]
fn embed_caps_hold_the_prefixes_with_the_model() {
    let caps = EmbedCaps {
        dims: Dims(768),
        max_batch: BatchMax(32),
        max_input: Tokens(8192),
        prompts: EmbedPrompts {
            query: PrefixText("search_query: ".into()),
            document: PrefixText("search_document: ".into()),
        },
    };
    round_trip(
        &caps,
        r#"{"dims":768,"max_batch":32,"max_input":8192,"prompts":{"query":"search_query: ","document":"search_document: "}}"#,
    );
}

#[test]
fn embed_faults_round_trip() {
    round_trip(
        &EmbedFault::CountMismatch {
            want: Count(4),
            got: Count(3),
        },
        r#"{"kind":"count_mismatch","v":{"want":4,"got":3}}"#,
    );
    round_trip(
        &EmbedFault::WidthMismatch {
            want: Dims(768),
            got: Dims(384),
        },
        r#"{"kind":"width_mismatch","v":{"want":768,"got":384}}"#,
    );
}

fn field(name: &str, shape: Shape) -> Field {
    Field {
        name: FieldName::new(name).unwrap(),
        shape,
    }
}

#[test]
fn every_shape_round_trips() {
    let shape = Shape::Record(vec![
        field(
            "verdict",
            Shape::Choice(vec![ChoiceText("allow".into()), ChoiceText("deny".into())]),
        ),
        field("score", Shape::Integer { min: 0, max: 10 }),
        field(
            "note",
            Shape::Optional(Box::new(Shape::Text {
                max: CharCount(200),
            })),
        ),
        field("on", Shape::Date),
        field("at", Shape::DateTime),
        field(
            "items",
            Shape::List {
                of: Box::new(Shape::Integer { min: -5, max: 5 }),
                max: Count(8),
            },
        ),
        field(
            "act",
            Shape::Tagged {
                tag: FieldName::new("kind").unwrap(),
                content: FieldName::new("v").unwrap(),
                variants: vec![Variant {
                    name: VariantName::new("archive").unwrap(),
                    shape: Shape::Date,
                }],
            },
        ),
    ]);
    let json = serde_json::to_string(&shape).unwrap();
    assert_eq!(serde_json::from_str::<Shape>(&json).unwrap(), shape);
    round_trip(&Shape::Date, r#"{"kind":"date"}"#);
    round_trip(
        &Shape::Integer { min: 0, max: 9 },
        r#"{"kind":"integer","v":{"min":0,"max":9}}"#,
    );
}

#[test]
fn field_and_variant_names_are_checked_where_they_enter() {
    assert!(FieldName::new("verdict_2").is_ok());
    assert!(FieldName::new("_x").is_ok());
    assert!(FieldName::new("").is_err());
    assert!(FieldName::new("2nd").is_err());
    assert!(FieldName::new("has space").is_err());
    assert!(FieldName::new("quote\"").is_err());
    assert!(FieldName::new("x".repeat(65)).is_err());
    assert!(serde_json::from_str::<VariantName>(r#""a-b""#).is_err());
}

#[test]
fn schema_dialects_and_shape_kinds_have_stable_slugs() {
    round_trip(&SchemaDialect::OpenAiStrict, r#""open_ai_strict""#);
    round_trip(&SchemaDialect::Anthropic, r#""anthropic""#);
    round_trip(&ShapeKind::DateTime, r#""date_time""#);
}

#[test]
fn retry_types_round_trip() {
    round_trip(&RetryClass::Never, r#"{"kind":"never"}"#);
    round_trip(&RetryClass::Transient, r#"{"kind":"transient"}"#);
    round_trip(
        &RetryClass::Overload(RetrySeconds(30)),
        r#"{"kind":"overload","v":30}"#,
    );
    round_trip(
        &RetryPolicy {
            attempts: Attempt(3),
            base: WaitMs(250),
            cap: WaitMs(8000),
        },
        r#"{"attempts":3,"base":250,"cap":8000}"#,
    );
    assert!(Permille(500) < Permille(501));
}

#[test]
fn reasoning_has_an_engine_default_arm_with_pinned_json() {
    round_trip(&Reasoning::EngineDefault, r#"{"kind":"engine_default"}"#);
    round_trip(&Reasoning::Off, r#"{"kind":"off"}"#);
    round_trip(
        &Reasoning::On(model_provider::Effort::High),
        r#"{"kind":"on","v":"high"}"#,
    );
}
