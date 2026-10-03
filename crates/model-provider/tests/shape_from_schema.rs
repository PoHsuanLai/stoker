//! `Shape::from_json_schema`: the subset it reads, the typed refusal of the rest, and the round
//! trip with `Shape::to_json_schema`.

use model_provider::{
    CharCount, ChoiceText, Count, Field, FieldName, JsonText, Refused, SchemaDialect, SchemaLimits,
    SchemaRefusal, SchemaText, Shape, Variant, VariantName,
};
use proptest::prelude::*;
use serde_json::{Value, json};

const LIMITS: SchemaLimits = SchemaLimits {
    open_text: CharCount(100),
    open_list: Count(10),
    depth: Count(6),
};

fn read(value: &Value) -> Result<Shape, SchemaRefusal> {
    let text = SchemaText(JsonText::new(value.to_string()).unwrap());
    Shape::from_json_schema(&text, LIMITS)
}

fn refused(value: &Value) -> (String, Refused) {
    let error = read(value).unwrap_err();
    (error.at.0, error.why)
}

fn name(text: &str) -> FieldName {
    FieldName::new(text).unwrap()
}

fn field(text: &str, shape: Shape) -> Field {
    Field {
        name: name(text),
        shape,
    }
}

#[test]
fn the_scalars_read() {
    assert_eq!(
        read(&json!({"type": "string", "enum": ["allow", "deny"]})),
        Ok(Shape::Choice(vec![
            ChoiceText("allow".into()),
            ChoiceText("deny".into())
        ]))
    );
    assert_eq!(
        read(&json!({"enum": ["a"]})),
        Ok(Shape::Choice(vec![ChoiceText("a".into())]))
    );
    assert_eq!(
        read(&json!({"const": "a"})),
        Ok(Shape::Choice(vec![ChoiceText("a".into())]))
    );
    assert_eq!(
        read(&json!({"type": "integer", "minimum": -2, "maximum": 9})),
        Ok(Shape::Integer { min: -2, max: 9 })
    );
    assert_eq!(
        read(&json!({"type": "integer", "exclusiveMinimum": 0, "exclusiveMaximum": 10})),
        Ok(Shape::Integer { min: 1, max: 9 })
    );
    assert_eq!(
        read(&json!({"type": "integer"})),
        Ok(Shape::Integer {
            min: i64::MIN,
            max: i64::MAX
        })
    );
    assert_eq!(
        read(&json!({"type": "string", "maxLength": 12})),
        Ok(Shape::Text { max: CharCount(12) })
    );
    assert_eq!(
        read(&json!({"type": "string"})),
        Ok(Shape::Text {
            max: CharCount(100)
        })
    );
    assert_eq!(
        read(&json!({"type": "string", "format": "date"})),
        Ok(Shape::Date)
    );
    assert_eq!(
        read(&json!({"type": "string", "format": "date-time"})),
        Ok(Shape::DateTime)
    );
}

#[test]
fn annotations_are_ignored() {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Verdict", "description": "d", "default": "allow", "examples": ["allow"],
        "type": "string", "enum": ["allow"]
    });
    assert_eq!(
        read(&schema),
        Ok(Shape::Choice(vec![ChoiceText("allow".into())]))
    );
}

#[test]
fn a_record_lists_its_properties_in_name_order_and_optional_is_not_required() {
    let schema = json!({
        "type": "object",
        "properties": {
            "verdict": {"type": "string", "enum": ["allow", "deny"]},
            "note": {"type": "string", "maxLength": 3},
            "tags": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 1}, "maxItems": 2},
            "when": {"anyOf": [{"type": "string", "format": "date"}, {"type": "null"}]},
            "nick": {"type": ["string", "null"], "maxLength": 4}
        },
        "required": ["verdict", "tags"],
        "additionalProperties": false
    });
    let shape = read(&schema).unwrap();
    assert_eq!(
        shape,
        Shape::Record(vec![
            field(
                "nick",
                Shape::Optional(Box::new(Shape::Text { max: CharCount(4) }))
            ),
            field(
                "note",
                Shape::Optional(Box::new(Shape::Text { max: CharCount(3) }))
            ),
            field(
                "tags",
                Shape::List {
                    of: Box::new(Shape::Integer { min: 0, max: 1 }),
                    max: Count(2)
                }
            ),
            field(
                "verdict",
                Shape::Choice(vec![ChoiceText("allow".into()), ChoiceText("deny".into())])
            ),
            field("when", Shape::Optional(Box::new(Shape::Date))),
        ])
    );
    // The shape is what the repair loop checks a reply with.
    let reply = JsonText::new(r#"{"verdict":"allow","tags":[1]}"#).unwrap();
    assert_eq!(shape.check(&reply), Ok(()));
    let bad = JsonText::new(r#"{"verdict":"maybe","tags":[]}"#).unwrap();
    assert!(shape.check(&bad).is_err());
}

#[test]
fn a_tagged_enum_reads_whichever_property_holds_the_tag() {
    let arm = |tag: &str, content: Value| {
        json!({"type": "object",
            "properties": {"kind": {"type": "string", "enum": [tag]}, "v": content},
            "required": ["kind", "v"], "additionalProperties": false})
    };
    let schema = json!({"oneOf": [
        arm("point", json!({"type": "integer", "minimum": 0, "maximum": 9})),
        arm("note", json!({"type": "string", "maxLength": 5})),
    ]});
    let want = Shape::Tagged {
        tag: name("kind"),
        content: name("v"),
        variants: vec![
            Variant {
                name: VariantName::new("point").unwrap(),
                shape: Shape::Integer { min: 0, max: 9 },
            },
            Variant {
                name: VariantName::new("note").unwrap(),
                shape: Shape::Text { max: CharCount(5) },
            },
        ],
    };
    assert_eq!(read(&schema), Ok(want.clone()));
    // The strict dialect writes `anyOf` for `oneOf`; it reads the same.
    let any = json!({"anyOf": schema["oneOf"].clone()});
    assert_eq!(read(&any), Ok(want));
    // Two one-string properties in one arm are ambiguous, and so is one arm alone.
    let both = json!({"oneOf": [
        arm("point", json!({"type": "string", "enum": ["x"]})),
        arm("note", json!({"type": "string", "enum": ["y"]})),
    ]});
    assert_eq!(refused(&both), (String::new(), Refused::BadTagged));
    // An arm with a different pair of names, a stray property or no arms at all is refused.
    let other = json!({"type": "object",
        "properties": {"t": {"enum": ["z"]}, "v": {"type": "integer"}},
        "required": ["t", "v"], "additionalProperties": false});
    let mixed = json!({"oneOf": [arm("a", json!({"type": "integer"})), other]});
    assert_eq!(refused(&mixed).1, Refused::BadTagged);
    assert_eq!(refused(&json!({"oneOf": []})).1, Refused::BadTagged);
}

#[test]
fn what_the_vocabulary_cannot_say_is_refused_with_a_path_and_a_reason() {
    use Refused::*;
    let keyword = |k: &str| Unsupported(model_provider::KeywordText(k.into()));
    let object = |props: Value| json!({"type": "object", "properties": props, "required": [], "additionalProperties": false});
    let cases: Vec<(Value, &str, Refused)> = vec![
        (json!(true), "", NotASchema),
        (json!({}), "", NoType),
        (json!({"type": "number"}), "", UnsupportedType),
        (json!({"type": "boolean"}), "", UnsupportedType),
        (json!({"type": "null"}), "", UnsupportedType),
        (json!({"type": ["string", "integer"]}), "", UnsupportedType),
        (
            json!({"type": "string", "pattern": "^a"}),
            "",
            keyword("pattern"),
        ),
        (
            json!({"type": "string", "minLength": 1}),
            "",
            keyword("minLength"),
        ),
        (
            json!({"type": "string", "format": "email"}),
            "",
            keyword("format"),
        ),
        (json!({"$ref": "#/$defs/a"}), "", keyword("$ref")),
        (
            json!({"type": "object", "additionalProperties": false, "properties": {}, "$defs": {}}),
            "",
            keyword("$defs"),
        ),
        (json!({"allOf": [{"type": "string"}]}), "", keyword("allOf")),
        (json!({"enum": []}), "", BadEnum),
        (json!({"enum": [1, 2]}), "", BadEnum),
        (json!({"type": "integer", "minimum": 1.5}), "", BadBound),
        (
            json!({"type": "integer", "minimum": 5, "maximum": 1}),
            "",
            BadBound,
        ),
        (
            json!({"type": "integer", "exclusiveMinimum": i64::MAX}),
            "",
            BadBound,
        ),
        (json!({"type": "string", "maxLength": -1}), "", BadBound),
        (
            json!({"type": "array", "items": {"type": "string"}, "minItems": 1}),
            "",
            keyword("minItems"),
        ),
        (
            json!({"type": "array", "items": {"type": "string"}, "uniqueItems": true}),
            "",
            keyword("uniqueItems"),
        ),
        (json!({"type": "array"}), "", NoType),
        (json!({"type": "object"}), "", OpenObject),
        (
            json!({"type": "object", "additionalProperties": true}),
            "",
            OpenObject,
        ),
        (
            object(json!({"a-b": {"type": "string"}})),
            "/properties/a-b",
            BadName,
        ),
        (
            json!({"type": "object", "properties": {}, "required": ["x"], "additionalProperties": false}),
            "",
            BadName,
        ),
        (
            object(json!({"a": {"type": "number"}})),
            "/properties/a",
            UnsupportedType,
        ),
        (
            json!({"type": "array", "items": object(json!({"a": {"type": "array", "items": {"type": "boolean"}}}))}),
            "/items/properties/a/items",
            UnsupportedType,
        ),
        (
            json!({"anyOf": [{"type": "string"}, {"type": "integer"}]}),
            "",
            BadTagged,
        ),
    ];
    for (schema, at, why) in cases {
        assert_eq!(refused(&schema), (at.to_owned(), why), "{schema}");
    }
}

#[test]
fn nesting_is_bounded_by_the_limit() {
    let mut schema = json!({"type": "string", "maxLength": 1});
    for _ in 0..5 {
        schema = json!({"type": "array", "items": schema, "maxItems": 1});
    }
    assert!(read(&schema).is_ok());
    schema = json!({"type": "array", "items": schema, "maxItems": 1});
    let (at, why) = refused(&schema);
    assert_eq!(why, Refused::TooDeep);
    assert_eq!(at.matches("/items").count(), 6);
    // A schema far deeper than the stack could hold is refused, not walked.
    let mut deep = json!({"type": "string"});
    for _ in 0..40 {
        deep = json!({"anyOf": [deep, {"type": "null"}]});
    }
    assert_eq!(refused(&deep).1, Refused::TooDeep);
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

/// A shape whose content is not a one-string choice (that is the one ambiguous tagged form).
fn content() -> impl Strategy<Value = Shape> {
    leaf().prop_filter(
        "a one-string choice is ambiguous",
        |s| !matches!(s, Shape::Choice(items) if items.len() == 1),
    )
}

fn shape() -> impl Strategy<Value = Shape> {
    leaf().prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            (inner.clone(), 0_u32..3).prop_map(|(of, n)| Shape::List {
                of: Box::new(of),
                max: Count(n)
            }),
            inner.clone().prop_map(|s| Shape::Optional(Box::new(s))),
            proptest::collection::vec(inner.clone(), 0..3).prop_map(|shapes| Shape::Record(
                shapes
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| field(&format!("f{i}"), s))
                    .collect()
            )),
            (content(), inner).prop_map(|(a, b)| Shape::Tagged {
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
        ]
    })
}

proptest! {
    #[test]
    fn what_to_json_schema_writes_reads_back_to_the_same_shape(shape in shape()) {
        // The generator keeps variant `a` clear of a one-string choice, the one form that makes
        // the tag ambiguous, so every shape it makes reads back.
        for dialect in [SchemaDialect::Plain, SchemaDialect::OpenAiStrict] {
            let schema = shape.to_json_schema(dialect);
            let back = Shape::from_json_schema(&schema, LIMITS);
            prop_assert_eq!(back.as_ref(), Ok(&shape), "{} {:?}", schema.0.as_str(), dialect);
        }
    }

    #[test]
    fn a_reply_the_read_shape_accepts_fits_the_original_schema_text(shape in shape()) {
        let schema = shape.to_json_schema(SchemaDialect::Plain);
        if let Ok(read) = Shape::from_json_schema(&schema, LIMITS) {
            prop_assert_eq!(read.to_json_schema(SchemaDialect::Plain), schema);
        }
    }

    #[test]
    fn arbitrary_json_is_read_or_refused_never_a_panic(text in "[\\[\\]{}\":,a-z0-9 .-]{0,60}") {
        if let Ok(json) = JsonText::new(text) {
            let _ = Shape::from_json_schema(&SchemaText(json), LIMITS);
        }
    }
}
