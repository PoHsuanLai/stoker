//! `Shape` to JSON Schema, GBNF and regex.

mod support;

use model_provider::{
    CharCount, ChoiceText, Count, Field, FieldName, SchemaDialect, SchemaText, Shape, ShapeFault,
    Variant, VariantName, sanitize_schema,
};
use proptest::prelude::*;
use support::Grammar;

fn name(text: &str) -> FieldName {
    FieldName::new(text).unwrap()
}

fn field(text: &str, shape: Shape) -> Field {
    Field {
        name: name(text),
        shape,
    }
}

fn choice(items: &[&str]) -> Shape {
    Shape::Choice(items.iter().map(|s| ChoiceText((*s).into())).collect())
}

fn schema(shape: &Shape, dialect: SchemaDialect) -> serde_json::Value {
    serde_json::from_str(shape.to_json_schema(dialect).0.as_str()).unwrap()
}

fn verdict() -> Shape {
    Shape::Record(vec![
        field("verdict", choice(&["allow", "deny"])),
        field("score", Shape::Integer { min: 0, max: 10 }),
        field(
            "note",
            Shape::Optional(Box::new(Shape::Text {
                max: CharCount(200),
            })),
        ),
    ])
}

#[test]
fn scalar_schemas_are_pinned() {
    const CASES: &[(&str, &str)] = &[
        ("choice", r#"{"enum":["a","b"],"type":"string"}"#),
        ("integer", r#"{"maximum":9,"minimum":-2,"type":"integer"}"#),
        ("text", r#"{"maxLength":5,"type":"string"}"#),
        ("date", r#"{"format":"date","type":"string"}"#),
        ("date_time", r#"{"format":"date-time","type":"string"}"#),
        (
            "list",
            r#"{"items":{"type":"string","format":"date"},"maxItems":3,"type":"array"}"#,
        ),
    ];
    let shapes = [
        choice(&["a", "b"]),
        Shape::Integer { min: -2, max: 9 },
        Shape::Text { max: CharCount(5) },
        Shape::Date,
        Shape::DateTime,
        Shape::List {
            of: Box::new(Shape::Date),
            max: Count(3),
        },
    ];
    for ((label, want), shape) in CASES.iter().zip(&shapes) {
        let got = schema(shape, SchemaDialect::Plain);
        let want: serde_json::Value = serde_json::from_str(want).unwrap();
        assert_eq!(got, want, "{label}");
    }
}

#[test]
fn a_plain_record_requires_what_is_not_optional() {
    let got = schema(&verdict(), SchemaDialect::Plain);
    assert_eq!(got["required"], serde_json::json!(["verdict", "score"]));
    assert_eq!(got["additionalProperties"], serde_json::json!(false));
    assert_eq!(
        got["properties"]["note"]["anyOf"][1],
        serde_json::json!({"type": "null"})
    );
}

#[test]
fn the_strict_dialect_requires_every_property_and_turns_one_of_into_any_of() {
    let shape = Shape::Record(vec![
        field("note", Shape::Optional(Box::new(Shape::Date))),
        field("count", Shape::Integer { min: 0, max: 3 }),
        field(
            "act",
            Shape::Tagged {
                tag: name("kind"),
                content: name("v"),
                variants: vec![Variant {
                    name: VariantName::new("archive").unwrap(),
                    shape: Shape::Date,
                }],
            },
        ),
    ]);
    let got = schema(&shape, SchemaDialect::OpenAiStrict);
    assert_eq!(got["required"], serde_json::json!(["act", "count", "note"]));
    let act = &got["properties"]["act"];
    assert!(act.get("oneOf").is_none() && act["anyOf"][0]["additionalProperties"] == false);
    // The optional field accepts null once, not twice.
    assert_eq!(
        got["properties"]["note"]["anyOf"].as_array().unwrap().len(),
        2
    );
    // Plain keeps the original keyword.
    assert!(
        schema(&shape, SchemaDialect::Plain)["properties"]["act"]
            .get("oneOf")
            .is_some()
    );
}

#[test]
fn the_anthropic_dialect_strips_bounds_and_keeps_the_rest() {
    let got = schema(&verdict(), SchemaDialect::Anthropic);
    assert_eq!(
        got["properties"]["score"],
        serde_json::json!({"type": "integer"})
    );
    assert_eq!(got["required"], serde_json::json!(["verdict", "score"]));
    assert_eq!(
        got["properties"]["note"]["anyOf"][0],
        serde_json::json!({"type": "string"})
    );
    assert_eq!(got["additionalProperties"], serde_json::json!(false));
}

#[test]
fn a_ref_loses_its_siblings_under_the_strict_dialect_and_a_field_named_like_a_keyword_survives() {
    let input = SchemaText(
        model_provider::JsonText::new(
            r##"{"type":"object","properties":{"minimum":{"$ref":"#/$defs/n","description":"x"}},"$defs":{"n":{"type":"integer","oneOf":[{"type":"integer"}]}}}"##,
        )
        .unwrap(),
    );
    let out: serde_json::Value = serde_json::from_str(
        sanitize_schema(&input, SchemaDialect::OpenAiStrict)
            .0
            .as_str(),
    )
    .unwrap();
    assert_eq!(
        out["properties"]["minimum"],
        serde_json::json!({"anyOf":[{"$ref":"#/$defs/n"},{"type":"null"}]})
    );
    assert_eq!(out["required"], serde_json::json!(["minimum"]));
    assert!(out["$defs"]["n"].get("oneOf").is_none());
    let anthropic: serde_json::Value = serde_json::from_str(
        sanitize_schema(
            &SchemaText(
                model_provider::JsonText::new(
                    r#"{"type":"object","properties":{"minimum":{"type":"integer","minimum":1}}}"#,
                )
                .unwrap(),
            ),
            SchemaDialect::Anthropic,
        )
        .0
        .as_str(),
    )
    .unwrap();
    assert_eq!(
        anthropic["properties"]["minimum"],
        serde_json::json!({"type":"integer"})
    );
}

#[test]
fn regexes_exist_for_choice_and_bounded_integers_only() {
    const CASES: &[(&str, Option<&str>)] = &[
        ("choice", Some("allow|deny")),
        ("escaped choice", Some(r"a\.b|c\(d\)")),
        ("one digit", Some("[0-9]")),
        ("two digits", Some("[0-9]|[1-9][0-9]")),
        ("reversed", None),
        ("text", None),
    ];
    let shapes = [
        choice(&["allow", "deny"]),
        choice(&["a.b", "c(d)"]),
        Shape::Integer { min: 0, max: 9 },
        Shape::Integer { min: 0, max: 99 },
        Shape::Integer { min: 5, max: 1 },
        Shape::Text { max: CharCount(3) },
    ];
    for ((label, want), shape) in CASES.iter().zip(&shapes) {
        assert_eq!(shape.to_regex().as_deref(), *want, "{label}");
    }
    assert_eq!(Shape::Choice(vec![]).to_regex(), None);
}

#[test]
fn gbnf_refuses_what_it_cannot_say() {
    for shape in [
        choice(&[]),
        Shape::Integer { min: 2, max: 1 },
        Shape::Tagged {
            tag: name("k"),
            content: name("v"),
            variants: vec![],
        },
        Shape::Record(vec![field("on", choice(&[]))]),
        Shape::OrHandle(Box::new(choice(&[]))),
    ] {
        assert_eq!(shape.to_gbnf(), Err(ShapeFault::NotRepresentable));
    }
}

#[test]
fn a_date_grammar_reads_the_days_each_month_has() {
    const CASES: &[(&str, bool)] = &[
        ("2026-10-03", true),
        ("0000-01-01", true),
        ("9999-12-31", true),
        ("2026-01-31", true),
        ("2026-04-30", true),
        ("2026-04-31", false),
        ("2026-02-28", true),
        ("2026-02-30", false),
        ("2026-02-31", false),
        ("2026-13-01", false),
        ("2026-00-10", false),
        ("2026-10-00", false),
        ("2026-10-32", false),
        ("26-10-03", false),
        ("2026-1-03", false),
        ("2026-10-03T00:00:00Z", false),
        ("2026-10-03 ", false),
    ];
    let grammar = Grammar::parse(&Shape::Date.to_gbnf().unwrap());
    for (text, ok) in CASES {
        let quoted = format!("\"{text}\"");
        assert_eq!(grammar.accepts(&quoted), *ok, "{text}");
        assert!(!grammar.accepts(text), "unquoted {text}");
    }
}

#[test]
fn the_date_grammar_is_a_subset_of_the_checker_over_every_month_and_day() {
    let grammar = Grammar::parse(&Shape::Date.to_gbnf().unwrap());
    for year in ["1900", "2000", "2023", "2024"] {
        for month in 0..=13 {
            for day in 0..=32 {
                let text = format!("{year}-{month:02}-{day:02}");
                let json = model_provider::JsonText::new(format!("\"{text}\"")).unwrap();
                let checked = Shape::Date.check(&json).is_ok();
                let written = grammar.accepts(&format!("\"{text}\""));
                assert!(!written || checked, "{text}");
                // The one date the grammar leaves out is a leap day.
                assert!(written || !checked || text.ends_with("-02-29"), "{text}");
            }
        }
    }
}

#[test]
fn a_date_time_grammar_reads_the_clock_and_the_zone() {
    const CASES: &[(&str, bool)] = &[
        ("2026-10-03T19:46:00Z", true),
        ("2026-10-03T00:00:00Z", true),
        ("2026-10-03T23:59:59Z", true),
        ("2026-10-03T23:59:59.123456Z", true),
        ("2026-10-03T08:00:00+08:00", true),
        ("2026-10-03T08:00:00-05:30", true),
        ("2026-10-03T24:00:00Z", false),
        ("2026-10-03T23:60:00Z", false),
        ("2026-10-03T23:59:60Z", false),
        ("2026-10-03T23:59:59", false),
        ("2026-10-03T23:59:59.Z", false),
        ("2026-10-03T23:59Z", false),
        ("2026-10-03 23:59:59Z", false),
        ("2026-10-03T08:00:00+8:00", false),
        ("2026-10-03T08:00:00+24:00", false),
        ("2026-02-30T00:00:00Z", false),
        ("2026-10-03", false),
    ];
    let grammar = Grammar::parse(&Shape::DateTime.to_gbnf().unwrap());
    for (text, ok) in CASES {
        let quoted = format!("\"{text}\"");
        assert_eq!(grammar.accepts(&quoted), *ok, "{text}");
        let json = model_provider::JsonText::new(quoted).unwrap();
        // Whatever the grammar writes, the checker reads.
        assert!(!ok || Shape::DateTime.check(&json).is_ok(), "{text}");
    }
}

#[test]
fn dates_in_a_record_share_their_rules() {
    let shape = Shape::Record(vec![
        field("from", Shape::Date),
        field("to", Shape::Optional(Box::new(Shape::Date))),
        field("at", Shape::DateTime),
    ]);
    let text = shape.to_gbnf().unwrap();
    assert_eq!(text.matches("\nday28 ::=").count(), 1);
    assert_eq!(text.matches("\ndate ::=").count(), 1);
    let grammar = Grammar::parse(&text);
    assert!(grammar.accepts(r#"{"from":"2026-10-03","to":null,"at":"2026-10-03T19:46:00Z"}"#));
    assert!(!grammar.accepts(r#"{"from":"2026-10-03","to":"x","at":"2026-10-03T19:46:00Z"}"#));
}

fn tagged() -> Shape {
    Shape::Tagged {
        tag: name("kind"),
        content: name("v"),
        variants: vec![
            Variant {
                name: VariantName::new("archive").unwrap(),
                shape: Shape::Date,
            },
            Variant {
                name: VariantName::new("snooze").unwrap(),
                shape: Shape::Integer { min: 1, max: 9 },
            },
        ],
    }
}

#[test]
fn a_tagged_grammar_writes_the_tag_first_and_pairs_it_with_its_content() {
    let grammar = Grammar::parse(&tagged().to_gbnf().unwrap());
    assert!(grammar.accepts(r#"{"kind":"archive","v":"2026-10-03"}"#));
    assert!(grammar.accepts(r#"{"kind": "snooze", "v": 4}"#));
    // The content of the other variant, an unknown tag, a missing member, a different order.
    assert!(!grammar.accepts(r#"{"kind":"archive","v":4}"#));
    assert!(!grammar.accepts(r#"{"kind":"snooze","v":"2026-10-03"}"#));
    assert!(!grammar.accepts(r#"{"kind":"delete","v":4}"#));
    assert!(!grammar.accepts(r#"{"kind":"snooze"}"#));
    assert!(!grammar.accepts(r#"{"v":4,"kind":"snooze"}"#));
}

#[test]
fn an_or_handle_grammar_takes_the_inner_shape_or_a_handle() {
    let shape = Shape::OrHandle(Box::new(Shape::Text { max: CharCount(5) }));
    let grammar = Grammar::parse(&shape.to_gbnf().unwrap());
    for ok in [
        r#""abc""#,
        r#"{"handle":0}"#,
        r#"{"handle":123}"#,
        r#"{ "handle" : 7 }"#,
        r#"{"handle":9223372036854775807}"#,
    ] {
        assert!(grammar.accepts(ok), "{ok}");
        let json = model_provider::JsonText::new(ok.replace(' ', "")).unwrap();
        assert_eq!(shape.check(&json), Ok(()), "{ok}");
    }
    for bad in [
        r#"{"handle":-1}"#,
        r#"{"handle":01}"#,
        r#"{"handle":1.5}"#,
        r#"{"handle":"1"}"#,
        r#"{"handle":1,"x":2}"#,
        r#"{}"#,
        "3",
    ] {
        assert!(!grammar.accepts(bad), "{bad}");
    }
}

#[test]
fn an_or_handle_schema_is_the_inner_schema_or_the_handle_object() {
    let shape = Shape::OrHandle(Box::new(Shape::Text { max: CharCount(5) }));
    let handle = serde_json::json!({
        "type": "object",
        "properties": {"handle": {"type": "integer", "minimum": 0}},
        "required": ["handle"],
        "additionalProperties": false,
    });
    assert_eq!(
        schema(&shape, SchemaDialect::Plain),
        serde_json::json!({"anyOf": [{"type": "string", "maxLength": 5}, handle]})
    );
    // The strict dialect keeps the alternatives (it only turns `oneOf` into `anyOf`).
    assert_eq!(
        schema(&shape, SchemaDialect::OpenAiStrict),
        schema(&shape, SchemaDialect::Plain)
    );
    // An entity or a handle, and a list of them: what a planner argument looks like.
    let entity = Shape::Record(vec![
        field("app", Shape::Text { max: CharCount(20) }),
        field("kind", choice(&["file"])),
        field("key", Shape::Text { max: CharCount(80) }),
    ]);
    let many = Shape::List {
        of: Box::new(Shape::OrHandle(Box::new(entity))),
        max: Count(8),
    };
    let got = schema(&many, SchemaDialect::Plain);
    assert_eq!(got["items"]["anyOf"][1], handle);
    assert_eq!(
        got["items"]["anyOf"][0]["required"],
        serde_json::json!(["app", "kind", "key"])
    );
}

#[test]
fn a_choice_grammar_accepts_exactly_its_strings() {
    let grammar = Grammar::parse(&choice(&["allow", "de\"ny"]).to_gbnf().unwrap());
    assert!(grammar.accepts(r#""allow""#) && grammar.accepts(r#""de\"ny""#));
    assert!(
        !grammar.accepts("\"allo\"") && !grammar.accepts("allow") && !grammar.accepts(r#""deny""#)
    );
}

#[test]
fn an_integer_grammar_accepts_exactly_the_range() {
    const RANGES: &[(i64, i64)] = &[
        (0, 0),
        (0, 9),
        (0, 10),
        (3, 7),
        (7, 12),
        (-5, 5),
        (-120, -3),
        (-9, -9),
        (95, 105),
        (0, 130),
        (-30, 130),
        (10, 99),
        (1, 100),
        // A negative range whose digit strings differ in their first digit: the sign applies to
        // every alternative (found by the proptest as -20..=-19).
        (-20, -19),
        (-2, -1),
        (-10, -9),
        (-100, -99),
        (-19, 20),
    ];
    for (min, max) in RANGES {
        let grammar = Grammar::parse(
            &Shape::Integer {
                min: *min,
                max: *max,
            }
            .to_gbnf()
            .unwrap(),
        );
        for n in -140..=140_i64 {
            assert_eq!(
                grammar.accepts(&n.to_string()),
                (*min..=*max).contains(&n),
                "{n} in {min}..={max}"
            );
        }
        for bad in ["01", "-0", "+1", "", "1.0", "--1"] {
            assert!(!grammar.accepts(bad), "{bad:?} in {min}..={max}");
        }
    }
}

#[test]
fn a_record_grammar_accepts_the_canonical_text_and_refuses_text_over_the_cap() {
    let shape = Shape::Record(vec![
        field("verdict", choice(&["allow", "deny"])),
        field("score", Shape::Integer { min: 0, max: 10 }),
        field(
            "note",
            Shape::Optional(Box::new(Shape::Text { max: CharCount(3) })),
        ),
        field(
            "tags",
            Shape::List {
                of: Box::new(Shape::Integer { min: 0, max: 1 }),
                max: Count(2),
            },
        ),
    ]);
    let grammar = Grammar::parse(&shape.to_gbnf().unwrap());
    let ok = [
        r#"{"verdict":"allow","score":7,"note":"abc","tags":[0,1]}"#,
        r#"{ "verdict" : "deny", "score": 10, "note": null, "tags": [] }"#,
        "{\"verdict\":\"deny\",\"score\":0,\"note\":\"\",\"tags\":[1]}",
    ];
    for text in ok {
        assert!(grammar.accepts(text), "{text}");
    }
    let bad = [
        r#"{"verdict":"allow","score":11,"note":null,"tags":[]}"#,
        r#"{"verdict":"allow","score":1,"note":"abcd","tags":[]}"#,
        r#"{"verdict":"allow","score":1,"note":null,"tags":[0,0,0]}"#,
        r#"{"score":1,"verdict":"allow","note":null,"tags":[]}"#,
    ];
    for text in bad {
        assert!(!grammar.accepts(text), "{text}");
    }
}

#[test]
fn an_empty_record_and_a_zero_length_list_have_grammars() {
    let record = Grammar::parse(&Shape::Record(vec![]).to_gbnf().unwrap());
    assert!(record.accepts("{}") && record.accepts("{ }") && !record.accepts("[]"));
    let list = Grammar::parse(
        &Shape::List {
            of: Box::new(Shape::Integer { min: 0, max: 1 }),
            max: Count(0),
        }
        .to_gbnf()
        .unwrap(),
    );
    assert!(list.accepts("[]") && !list.accepts("[1]"));
}

fn leaf() -> impl Strategy<Value = Shape> {
    prop_oneof![
        proptest::collection::vec("[a-z\"\\\\ é]{0,4}", 1..4)
            .prop_map(|items| Shape::Choice(items.into_iter().map(ChoiceText).collect())),
        (-20_i64..20, 0_i64..30).prop_map(|(a, w)| Shape::Integer { min: a, max: a + w }),
        (0_u32..6).prop_map(|n| Shape::Text { max: CharCount(n) }),
        Just(Shape::Date),
        Just(Shape::DateTime),
    ]
}

fn shape() -> impl Strategy<Value = Shape> {
    leaf().prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            inner.clone().prop_map(|s| Shape::OrHandle(Box::new(s))),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| Shape::Tagged {
                tag: name("kind"),
                content: name("v"),
                variants: vec![
                    Variant {
                        name: VariantName::new("a").unwrap(),
                        shape: a
                    },
                    Variant {
                        name: VariantName::new("b").unwrap(),
                        shape: b
                    },
                ],
            }),
            (inner.clone(), 0_u32..3).prop_map(|(of, n)| Shape::List {
                of: Box::new(of),
                max: Count(n)
            }),
            inner.clone().prop_map(|s| Shape::Optional(Box::new(s))),
            proptest::collection::vec(inner, 0..3).prop_map(|shapes| Shape::Record(
                shapes
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| field(&format!("f{i}"), s))
                    .collect()
            )),
        ]
    })
}

/// A canonical value of `shape`, chosen by `pick`.
fn sample(shape: &Shape, pick: &mut impl FnMut(usize) -> usize) -> serde_json::Value {
    use serde_json::{Value, json};
    match shape {
        Shape::Choice(items) => json!(items[pick(items.len())].0),
        Shape::Integer { min, max } => {
            json!(min + i64::try_from(pick((max - min + 1) as usize)).unwrap())
        }
        Shape::Text { max } => {
            let chars = ["a", "\"", "é", "\\", " ", "\n"];
            Value::String(
                (0..pick(max.0 as usize + 1))
                    .map(|_| chars[pick(chars.len())])
                    .collect(),
            )
        }
        Shape::Optional(inner) => {
            if pick(2) == 0 {
                Value::Null
            } else {
                sample(inner, pick)
            }
        }
        Shape::List { of, max } => Value::Array(
            (0..pick(max.0 as usize + 1))
                .map(|_| sample(of, pick))
                .collect(),
        ),
        Shape::Record(fields) => Value::Object(
            fields
                .iter()
                .map(|f| (f.name.as_str().to_owned(), sample(&f.shape, pick)))
                .collect(),
        ),
        // No 29 February: the grammar leaves leap days to the checker.
        Shape::Date => json!(["2026-10-03", "0000-01-31", "2024-02-28", "1999-12-31"][pick(4)]),
        Shape::DateTime => json!(
            [
                "2026-10-03T19:46:00Z",
                "2024-02-28T23:59:59.5+08:00",
                "1999-12-31T00:00:00-05:30"
            ][pick(3)]
        ),
        Shape::Tagged {
            tag,
            content,
            variants,
        } => {
            let chosen = &variants[pick(variants.len())];
            json!({ tag.as_str(): chosen.name.as_str(), content.as_str(): sample(&chosen.shape, pick) })
        }
        Shape::OrHandle(inner) => match pick(3) {
            0 => json!({"handle": pick(1000)}),
            _ => sample(inner, pick),
        },
    }
}

fn in_declared_order(shape: &Shape, value: &serde_json::Value) -> String {
    use serde_json::Value;
    match (shape, value) {
        (Shape::Record(fields), Value::Object(map)) => {
            let members: Vec<String> = fields
                .iter()
                .map(|f| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(f.name.as_str()).unwrap(),
                        in_declared_order(&f.shape, &map[f.name.as_str()])
                    )
                })
                .collect();
            format!("{{{}}}", members.join(","))
        }
        (Shape::List { of, .. }, Value::Array(items)) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|i| in_declared_order(of, i))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        (Shape::Optional(inner), v) if !v.is_null() => in_declared_order(inner, v),
        (Shape::OrHandle(inner), v) => match v.as_object() {
            Some(map) if map.contains_key("handle") => v.to_string(),
            _ => in_declared_order(inner, v),
        },
        (
            Shape::Tagged {
                tag,
                content,
                variants,
            },
            Value::Object(map),
        ) => {
            let at = map[tag.as_str()].as_str().unwrap_or_default();
            let variant = variants.iter().find(|v| v.name.as_str() == at).unwrap();
            format!(
                "{{{}:{},{}:{}}}",
                serde_json::to_string(tag.as_str()).unwrap(),
                map[tag.as_str()],
                serde_json::to_string(content.as_str()).unwrap(),
                in_declared_order(&variant.shape, &map[content.as_str()])
            )
        }
        (_, v) => v.to_string(),
    }
}

proptest! {
    #[test]
    fn a_value_the_checker_accepts_is_accepted_by_the_grammar(shape in shape(), seed in proptest::collection::vec(any::<usize>(), 64)) {
        let mut cursor = 0;
        let mut pick = |n: usize| { cursor += 1; seed[cursor % seed.len()] % n.max(1) };
        let value = sample(&shape, &mut pick);
        let text = in_declared_order(&shape, &value);
        let json = model_provider::JsonText::new(text.clone()).unwrap();
        prop_assert_eq!(shape.check(&json), Ok(()), "{}", text);
        let grammar = Grammar::parse(&shape.to_gbnf().unwrap());
        prop_assert!(grammar.accepts(&text), "{} against {}", text, shape.to_gbnf().unwrap());
    }
}
