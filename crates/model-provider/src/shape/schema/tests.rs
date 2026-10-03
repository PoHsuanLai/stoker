use serde_json::{Value, json};

use crate::SchemaDialect;

use super::sanitize_value;

fn tricky() -> Value {
    json!({
        "type": "object",
        "properties": {
            "maximum": {"type": "integer", "maximum": 5, "minimum": 1},
            "r": {"$ref": "#/$defs/r", "description": "d"},
            "list": {"type": "array", "items": {"type": "string", "maxLength": 3}, "maxItems": 2},
            "u": {"oneOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["r"],
        "$defs": {"r": {"type": "object", "properties": {"a": {"type": "number", "multipleOf": 2}}}}
    })
}

#[test]
fn plain_leaves_a_schema_alone() {
    assert_eq!(sanitize_value(tricky(), SchemaDialect::Plain), tricky());
}

#[test]
fn every_dialect_is_idempotent() {
    for dialect in [SchemaDialect::OpenAiStrict, SchemaDialect::Anthropic] {
        let once = sanitize_value(tricky(), dialect);
        assert_eq!(sanitize_value(once.clone(), dialect), once, "{dialect:?}");
    }
}

#[test]
fn strict_reaches_every_nested_object_and_array_element() {
    let out = sanitize_value(tricky(), SchemaDialect::OpenAiStrict);
    // `r` was required, so it stays a bare reference with its siblings stripped.
    assert_eq!(out["properties"]["r"], json!({"$ref": "#/$defs/r"}));
    assert_eq!(out["$defs"]["r"]["additionalProperties"], json!(false));
    assert_eq!(out["$defs"]["r"]["required"], json!(["a"]));
    // `u` was optional and already took null: its oneOf became anyOf, with no second null.
    assert!(out["properties"]["u"].get("oneOf").is_none());
    assert_eq!(
        out["properties"]["u"]["anyOf"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(out["required"], json!(["list", "maximum", "r", "u"]));
    assert_eq!(
        out["properties"]["maximum"]["anyOf"][0]["maximum"],
        json!(5)
    );
}

#[test]
fn anthropic_strips_bounds_at_every_depth_but_not_a_property_named_like_one() {
    let out = sanitize_value(tricky(), SchemaDialect::Anthropic);
    assert_eq!(out["properties"]["maximum"], json!({"type": "integer"}));
    assert_eq!(
        out["properties"]["list"],
        json!({"type": "array", "items": {"type": "string"}})
    );
    assert_eq!(
        out["$defs"]["r"]["properties"]["a"],
        json!({"type": "number"})
    );
    assert_eq!(out["$defs"]["r"]["additionalProperties"], json!(false));
}
