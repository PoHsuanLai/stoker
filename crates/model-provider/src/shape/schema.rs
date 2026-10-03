//! `Shape` to JSON Schema, and the per-provider sanitiser.
//!
//! The sanitiser ports the options of rig's `providers/internal/schema.rs` (MIT, see
//! THIRD-PARTY-NOTICES) and is public because tool parameters are schemas too.
//!
// Portions adapted from rig (https://github.com/0xPlaygrounds/rig, crates/rig-core, commit acdcf34),
// MIT License, Copyright (c) 2024, Playgrounds Analytics Inc. See THIRD-PARTY-NOTICES.

use serde_json::{Map, Value, json};

use crate::{JsonText, SchemaDialect, SchemaText, Shape};

/// Keywords Anthropic's structured output refuses: numeric bounds, string lengths and array
/// bounds (its docs: "minimum, maximum, multipleOf; minLength, maxLength; minItems beyond 0 or
/// 1, maxItems").
const ANTHROPIC_STRIPPED: &[&str] = &[
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "maxItems",
];

impl Shape {
    pub(crate) fn schema_value(&self) -> Value {
        match self {
            Shape::Choice(choices) => {
                let values: Vec<&str> = choices.iter().map(|c| c.0.as_str()).collect();
                json!({"type": "string", "enum": values})
            }
            Shape::Integer { min, max } => {
                json!({"type": "integer", "minimum": min, "maximum": max})
            }
            Shape::Text { max } => json!({"type": "string", "maxLength": max.0}),
            Shape::Date => json!({"type": "string", "format": "date"}),
            Shape::DateTime => json!({"type": "string", "format": "date-time"}),
            Shape::Record(fields) => {
                let properties: Map<String, Value> = fields
                    .iter()
                    .map(|f| (f.name.as_str().to_owned(), f.shape.schema_value()))
                    .collect();
                let required: Vec<&str> = fields
                    .iter()
                    .filter(|f| !matches!(f.shape, Shape::Optional(_)))
                    .map(|f| f.name.as_str())
                    .collect();
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                })
            }
            Shape::List { of, max } => {
                json!({"type": "array", "items": of.schema_value(), "maxItems": max.0})
            }
            Shape::Optional(inner) => {
                json!({"anyOf": [inner.schema_value(), {"type": "null"}]})
            }
            Shape::Tagged {
                tag,
                content,
                variants,
            } => {
                let one_of: Vec<Value> = variants
                    .iter()
                    .map(|v| {
                        json!({
                            "type": "object",
                            "properties": {
                                tag.as_str(): {"type": "string", "enum": [v.name.as_str()]},
                                content.as_str(): v.shape.schema_value(),
                            },
                            "required": [tag.as_str(), content.as_str()],
                            "additionalProperties": false,
                        })
                    })
                    .collect();
                json!({"oneOf": one_of})
            }
            // The handle has no upper bound written: it is an index, not a quantity.
            Shape::OrHandle(inner) => json!({"anyOf": [inner.schema_value(), {
                "type": "object",
                "properties": {"handle": {"type": "integer", "minimum": 0}},
                "required": ["handle"],
                "additionalProperties": false,
            }]}),
        }
    }

    pub(crate) fn schema_text(&self, dialect: SchemaDialect) -> SchemaText {
        text_of(&sanitize_value(self.schema_value(), dialect))
    }
}

fn text_of(value: &Value) -> SchemaText {
    let text = value.to_string();
    // A `Value` always prints as JSON, so `new` cannot fail.
    SchemaText(JsonText::new(text).expect("a serde_json Value prints as valid JSON"))
}

/// Rewrites a schema for a provider: see [`SchemaDialect`]. Text that is not JSON comes back
/// unchanged. The input is parsed, never evaluated.
pub fn sanitize_schema(schema: &SchemaText, dialect: SchemaDialect) -> SchemaText {
    match serde_json::from_str::<Value>(schema.0.as_str()) {
        Ok(value) => text_of(&sanitize_value(value, dialect)),
        Err(_) => schema.clone(),
    }
}

pub(crate) fn sanitize_value(value: Value, dialect: SchemaDialect) -> Value {
    match dialect {
        SchemaDialect::Plain => value,
        SchemaDialect::OpenAiStrict => rewrite(value, &strict_node),
        SchemaDialect::Anthropic => rewrite(value, &anthropic_node),
    }
}

type NodeFn<'a> = &'a dyn Fn(Map<String, Value>) -> Map<String, Value>;

/// Applies `node` to every schema object, children first. Only schema positions are visited: a
/// property named `minimum` is a name, not a keyword.
fn rewrite(value: Value, node: NodeFn) -> Value {
    let Value::Object(mut map) = value else {
        return value;
    };
    for key in ["properties", "$defs", "definitions", "patternProperties"] {
        if let Some(Value::Object(named)) = map.remove(key) {
            let named = named
                .into_iter()
                .map(|(k, v)| (k, rewrite(v, node)))
                .collect();
            map.insert(key.into(), Value::Object(named));
        }
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(Value::Array(items)) = map.remove(key) {
            let items = items.into_iter().map(|v| rewrite(v, node)).collect();
            map.insert(key.into(), Value::Array(items));
        }
    }
    for key in [
        "items",
        "additionalProperties",
        "not",
        "if",
        "then",
        "else",
        "contains",
    ] {
        if let Some(child) = map.remove(key) {
            map.insert(key.into(), rewrite(child, node));
        }
    }
    Value::Object(node(map))
}

fn is_object_node(map: &Map<String, Value>) -> bool {
    map.get("type") == Some(&json!("object")) || map.contains_key("properties")
}

fn anthropic_node(mut map: Map<String, Value>) -> Map<String, Value> {
    if is_object_node(&map) {
        map.insert("additionalProperties".into(), Value::Bool(false));
    }
    ANTHROPIC_STRIPPED.iter().for_each(|k| {
        map.remove(*k);
    });
    map
}

fn strict_node(mut map: Map<String, Value>) -> Map<String, Value> {
    if let Some(reference) = map.remove("$ref") {
        // Siblings of `$ref` are refused, so only the reference stays.
        return Map::from_iter([("$ref".to_owned(), reference)]);
    }
    if let Some(Value::Array(mut one_of)) = map.remove("oneOf") {
        let mut any_of = match map.remove("anyOf") {
            Some(Value::Array(existing)) => existing,
            _ => Vec::new(),
        };
        any_of.append(&mut one_of);
        map.insert("anyOf".into(), Value::Array(any_of));
    }
    if is_object_node(&map) {
        map.insert("additionalProperties".into(), Value::Bool(false));
        require_all(&mut map);
    }
    map
}

/// Every property becomes required; one that was optional now accepts `null` instead.
fn require_all(map: &mut Map<String, Value>) {
    let was_required: Vec<String> = match map.get("required") {
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(|n| n.as_str())
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    };
    let Some(Value::Object(properties)) = map.get_mut("properties") else {
        map.insert("required".into(), Value::Array(Vec::new()));
        return;
    };
    let names: Vec<Value> = properties.keys().cloned().map(Value::String).collect();
    for (name, schema) in properties.iter_mut() {
        if !was_required.contains(name) {
            *schema = nullable(std::mem::take(schema));
        }
    }
    map.insert("required".into(), Value::Array(names));
}

fn nullable(schema: Value) -> Value {
    let accepts_null = json!({"type": "null"});
    let already = match schema.get("anyOf") {
        Some(Value::Array(options)) => options.contains(&accepts_null),
        _ => schema.get("type") == Some(&json!("null")),
    };
    if already {
        schema
    } else {
        json!({"anyOf": [schema, accepts_null]})
    }
}

#[cfg(test)]
mod tests;
