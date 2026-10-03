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
        Shape::Date,
        Shape::DateTime,
        choice(&[]),
        Shape::Integer { min: 2, max: 1 },
    ] {
        assert_eq!(shape.to_gbnf(), Err(ShapeFault::NotRepresentable));
    }
    let tagged = Shape::Tagged {
        tag: name("k"),
        content: name("v"),
        variants: vec![],
    };
    assert_eq!(tagged.to_gbnf(), Err(ShapeFault::NotRepresentable));
    assert_eq!(
        Shape::Record(vec![field("on", Shape::Date)]).to_gbnf(),
        Err(ShapeFault::NotRepresentable)
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
    ]
}

fn shape() -> impl Strategy<Value = Shape> {
    leaf().prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
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
        Shape::Date | Shape::DateTime | Shape::Tagged { .. } => Value::Null,
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
