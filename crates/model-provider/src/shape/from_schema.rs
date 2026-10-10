//! A `Shape` from JSON-schema text: the subset the vocabulary can say, and a typed refusal of the
//! rest.
//!
//! The reader exists so that a caller that holds a schema as text (porter's `ReplyShape::Json`)
//! can still validate a reply with `Shape::check` and repair it through `ExtractSession`. A schema
//! the vocabulary cannot say is refused, never approximated: a looser `Shape` would pass a reply
//! the schema forbids, a stricter one would repair a reply it allows.
//!
//! What reads (everything `Shape::to_json_schema` writes, in the plain and the strict dialect,
//! reads back to the same shape):
//!
//! | Schema | Shape |
//! | --- | --- |
//! | `{"type":"string","enum":[..]}`, `{"enum":[..]}` or `{"const":".."}` of strings | `Choice` |
//! | `{"type":"integer"}` with `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` | `Integer` (a bound left out is the `i64` edge) |
//! | `{"type":"string"}` with `maxLength` | `Text` (a length left out is `SchemaLimits::open_text`) |
//! | `{"type":"string","format":"date"}` or `"date-time"` | `Date`, `DateTime` |
//! | `{"type":"array","items":S}` with `maxItems` | `List` (a count left out is `SchemaLimits::open_list`) |
//! | `{"type":"object","properties":..,"required":[..],"additionalProperties":false}` | `Record`, fields in the order of the sorted property names; a property not required is `Optional` |
//! | `{"anyOf":[S,{"type":"null"}]}` or `{"type":["T","null"]}` | `Optional` |
//! | `{"oneOf":[..]}` (or `anyOf`) of `{tag: {enum:[name]}, content: S}` objects | `Tagged` |
//! | `{"anyOf":[S,H]}` where `H` is `{"handle": integer from 0}` as an object with `additionalProperties: false` | `OrHandle` |
//!
//! Annotations (`title`, `description`, `default`, `examples`, `$schema`, `$id`, `$comment`,
//! `deprecated`, `readOnly`, `writeOnly`) are ignored; every other keyword the reader does not
//! consume (`pattern`, `$ref`, `allOf`, `minItems` above 0, an open object, a number, a boolean)
//! is a `SchemaRefusal`.

use core::fmt;

use serde_json::{Map, Value};

mod tagged;
use tagged::tagged;

use crate::{CharCount, ChoiceText, Count, Field, FieldName, SchemaText, Shape};

/// What a schema may leave open, and how deep it may nest. Settings the daemon reads: no default
/// lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemaLimits {
    /// The cap on a string whose schema states no `maxLength`.
    pub open_text: CharCount,
    /// The cap on an array whose schema states no `maxItems`.
    pub open_list: Count,
    /// How many levels of schema the reader descends (the root is the first).
    pub depth: Count,
}

/// Where in the schema, as a JSON pointer (`/properties/note/items`; empty for the root).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaPath(pub String);

impl fmt::Display for SchemaPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("the root")
        } else {
            f.write_str(&self.0)
        }
    }
}

/// A keyword of the schema the reader cannot honour.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeywordText(pub String);

/// Why one schema node is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refused {
    #[error("the text is not JSON")]
    NotJson,
    #[error("a schema is a JSON object here (a boolean schema has no shape)")]
    NotASchema,
    #[error("it states no type, enum or alternatives")]
    NoType,
    #[error("its type has no form in the vocabulary (numbers, booleans, null on its own)")]
    UnsupportedType,
    #[error("the keyword {0:?} cannot be checked")]
    Unsupported(KeywordText),
    #[error("a bound is not an integer, is negative where a count is meant, or is empty")]
    BadBound,
    #[error("an object must say `additionalProperties: false`")]
    OpenObject,
    #[error("a property name is not 1 to 64 characters of [A-Za-z0-9_], or `required` names none")]
    BadName,
    #[error("an enum is empty or holds something other than strings")]
    BadEnum,
    #[error("alternatives are not the tag and content objects of a tagged enum")]
    BadTagged,
    #[error("the schema nests deeper than the limit")]
    TooDeep,
}

/// A schema the vocabulary cannot say, and where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{at}: {why}")]
pub struct SchemaRefusal {
    pub at: SchemaPath,
    pub why: Refused,
}

const ANNOTATIONS: &[&str] = &[
    "title",
    "description",
    "$schema",
    "$id",
    "$comment",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];

type Found<T> = Result<T, SchemaRefusal>;

/// A schema node being read: its keywords, consumed as they are used, and where it is.
struct Node {
    map: Map<String, Value>,
    at: String,
    left: u32,
    limits: SchemaLimits,
}

impl Node {
    fn open(value: &Value, at: String, left: u32, limits: SchemaLimits) -> Found<Node> {
        let refuse = |why| SchemaRefusal {
            at: SchemaPath(at.clone()),
            why,
        };
        let Value::Object(map) = value else {
            return Err(refuse(Refused::NotASchema));
        };
        if left == 0 {
            return Err(refuse(Refused::TooDeep));
        }
        let mut map = map.clone();
        ANNOTATIONS.iter().for_each(|k| {
            map.remove(*k);
        });
        Ok(Node {
            map,
            at,
            left,
            limits,
        })
    }

    fn refuse(&self, why: Refused) -> SchemaRefusal {
        SchemaRefusal {
            at: SchemaPath(self.at.clone()),
            why,
        }
    }

    fn child(&self, value: &Value, step: &[&str]) -> Found<Node> {
        let at = step
            .iter()
            .fold(self.at.clone(), |at, s| format!("{at}/{s}"));
        Node::open(value, at, self.left - 1, self.limits)
    }

    fn take(&mut self, key: &str) -> Option<Value> {
        self.map.remove(key)
    }

    /// A keyword that is a non-negative integer.
    fn count(&mut self, key: &str) -> Found<Option<u32>> {
        self.take(key)
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| self.refuse(Refused::BadBound))
            })
            .transpose()
    }

    fn integer(&mut self, key: &str) -> Found<Option<i64>> {
        self.take(key)
            .map(|v| v.as_i64().ok_or_else(|| self.refuse(Refused::BadBound)))
            .transpose()
    }

    /// Nothing may be left: every other keyword is one the reader cannot check.
    fn done(self, shape: Shape) -> Found<Shape> {
        match self.map.keys().next() {
            None => Ok(shape),
            Some(key) => Err(self.refuse(Refused::Unsupported(KeywordText(key.clone())))),
        }
    }
}

impl Shape {
    /// The shape of a JSON schema, or why the vocabulary cannot say it. See the module docs for
    /// the subset. Total: any text is read or refused.
    pub fn from_json_schema(
        schema: &SchemaText,
        limits: SchemaLimits,
    ) -> Result<Shape, SchemaRefusal> {
        let value: Value = serde_json::from_str(schema.0.as_str()).map_err(|_| SchemaRefusal {
            at: SchemaPath(String::new()),
            why: Refused::NotJson,
        })?;
        read(Node::open(&value, String::new(), limits.depth.0, limits)?)
    }
}

fn read(mut node: Node) -> Found<Shape> {
    for key in ["anyOf", "oneOf"] {
        if let Some(alternatives) = node.take(key) {
            return alternatives_of(node, key, &alternatives);
        }
    }
    if let Some(values) = node.take("enum") {
        node.take_string_type();
        return choice(node, &values);
    }
    if let Some(value) = node.take("const") {
        node.take_string_type();
        return choice(node, &Value::Array(vec![value]));
    }
    match node.take("type") {
        Some(Value::String(name)) => typed(node, &name),
        Some(Value::Array(names)) => nullable_type(node, &names),
        Some(_) => Err(node.refuse(Refused::UnsupportedType)),
        None => Err(match node.map.keys().next() {
            Some(key) => node.refuse(Refused::Unsupported(KeywordText(key.clone()))),
            None => node.refuse(Refused::NoType),
        }),
    }
}

impl Node {
    /// `{"type":"string"}` beside an `enum` or a `const` says nothing more.
    fn take_string_type(&mut self) {
        if self.map.get("type") == Some(&Value::String("string".into())) {
            self.map.remove("type");
        }
    }
}

fn choice(node: Node, values: &Value) -> Found<Shape> {
    let items: Option<Vec<ChoiceText>> = values.as_array().and_then(|items| {
        items
            .iter()
            .map(|v| v.as_str().map(|s| ChoiceText(s.to_owned())))
            .collect()
    });
    match items {
        Some(items) if !items.is_empty() => node.done(Shape::Choice(items)),
        _ => Err(node.refuse(Refused::BadEnum)),
    }
}

fn typed(mut node: Node, name: &str) -> Found<Shape> {
    let shape = match name {
        "string" => string(&mut node)?,
        "integer" => integer(&mut node)?,
        "array" => array(&mut node)?,
        "object" => object(&mut node)?,
        _ => return Err(node.refuse(Refused::UnsupportedType)),
    };
    node.done(shape)
}

/// `{"type":["string","null"]}`: the one type, or null.
fn nullable_type(mut node: Node, names: &[Value]) -> Found<Shape> {
    let null = Value::String("null".into());
    let [first, second] = names else {
        return Err(node.refuse(Refused::UnsupportedType));
    };
    let name = match (first, second) {
        (other, n) | (n, other) if *n == null && *other != null => other,
        _ => return Err(node.refuse(Refused::UnsupportedType)),
    };
    node.map.insert("type".into(), name.clone());
    let at = node.at.clone();
    let inner = read(Node {
        map: node.map,
        at,
        left: node.left,
        limits: node.limits,
    })?;
    Ok(Shape::Optional(Box::new(inner)))
}

fn string(node: &mut Node) -> Found<Shape> {
    match node.take("format") {
        Some(Value::String(f)) if f == "date" => Ok(Shape::Date),
        Some(Value::String(f)) if f == "date-time" => Ok(Shape::DateTime),
        Some(_) => Err(node.refuse(Refused::Unsupported(KeywordText("format".into())))),
        None => {
            if node.count("minLength")?.is_some_and(|n| n > 0) {
                return Err(node.refuse(Refused::Unsupported(KeywordText("minLength".into()))));
            }
            let max = node.count("maxLength")?.unwrap_or(node.limits.open_text.0);
            Ok(Shape::Text {
                max: CharCount(max),
            })
        }
    }
}

fn integer(node: &mut Node) -> Found<Shape> {
    let low = node.integer("minimum")?;
    let high = node.integer("maximum")?;
    let above = node.integer("exclusiveMinimum")?.map(|n| n.checked_add(1));
    let below = node.integer("exclusiveMaximum")?.map(|n| n.checked_sub(1));
    let bound = |a: Option<i64>, b: Option<Option<i64>>, pick: fn(i64, i64) -> i64, edge: i64| {
        let b = match b {
            Some(None) => return None,
            Some(Some(n)) => Some(n),
            None => None,
        };
        Some(match (a, b) {
            (Some(a), Some(b)) => pick(a, b),
            (Some(n), None) | (None, Some(n)) => n,
            (None, None) => edge,
        })
    };
    let min = bound(low, above, i64::max, i64::MIN);
    let max = bound(high, below, i64::min, i64::MAX);
    match (min, max) {
        (Some(min), Some(max)) if min <= max => Ok(Shape::Integer { min, max }),
        _ => Err(node.refuse(Refused::BadBound)),
    }
}

fn array(node: &mut Node) -> Found<Shape> {
    if node.count("minItems")?.is_some_and(|n| n > 0) {
        return Err(node.refuse(Refused::Unsupported(KeywordText("minItems".into()))));
    }
    if node
        .take("uniqueItems")
        .is_some_and(|v| v != Value::Bool(false))
    {
        return Err(node.refuse(Refused::Unsupported(KeywordText("uniqueItems".into()))));
    }
    let max = node.count("maxItems")?.unwrap_or(node.limits.open_list.0);
    let items = node
        .take("items")
        .ok_or_else(|| node.refuse(Refused::NoType))?;
    let of = read(node.child(&items, &["items"])?)?;
    Ok(Shape::List {
        of: Box::new(of),
        max: Count(max),
    })
}

fn object(node: &mut Node) -> Found<Shape> {
    if node.take("additionalProperties") != Some(Value::Bool(false)) {
        return Err(node.refuse(Refused::OpenObject));
    }
    let required = match node.take("required") {
        None => Vec::new(),
        Some(Value::Array(names)) => names
            .into_iter()
            .map(|n| n.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| node.refuse(Refused::BadName))?,
        Some(_) => return Err(node.refuse(Refused::BadName)),
    };
    let properties = match node.take("properties") {
        None => Map::new(),
        Some(Value::Object(properties)) => properties,
        Some(_) => return Err(node.refuse(Refused::NotASchema)),
    };
    if required.iter().any(|name| !properties.contains_key(name)) {
        return Err(node.refuse(Refused::BadName));
    }
    let fields = properties
        .iter()
        .map(|(name, schema)| {
            let name_ok = FieldName::new(name.clone());
            let child = node.child(schema, &["properties", name])?;
            let name = name_ok.map_err(|_| child.refuse(Refused::BadName))?;
            let shape = read(child)?;
            let shape = match shape {
                optional @ Shape::Optional(_) => optional,
                shape if required.contains(&name.as_str().to_owned()) => shape,
                shape => Shape::Optional(Box::new(shape)),
            };
            Ok(Field { name, shape })
        })
        .collect::<Found<Vec<_>>>()?;
    Ok(Shape::Record(fields))
}

/// `anyOf` or `oneOf`: an optional (one alternative is null), a shape or a handle, or a tagged enum.
fn alternatives_of(mut node: Node, key: &str, alternatives: &Value) -> Found<Shape> {
    let Value::Array(items) = alternatives else {
        return Err(node.refuse(Refused::BadTagged));
    };
    let null = |v: &Value| {
        v.as_object().is_some_and(|m| {
            m.get("type") == Some(&Value::String("null".into()))
                && m.keys()
                    .all(|k| k == "type" || ANNOTATIONS.contains(&k.as_str()))
        })
    };
    let others: Vec<(usize, &Value)> = items.iter().enumerate().filter(|(_, v)| !null(v)).collect();
    let shape = match (items.len(), &others[..]) {
        (2, [(at, other)]) => {
            let inner = read(node.child(other, &[key, &at.to_string()])?)?;
            Shape::Optional(Box::new(inner))
        }
        _ => match or_handle(&node, key, items) {
            Some(shape) => shape,
            None => tagged(&mut node, key, items)?,
        },
    };
    node.done(shape)
}

/// Two alternatives of which exactly one is a handle (the other reads as a shape of its own).
fn or_handle(node: &Node, key: &str, items: &[Value]) -> Option<Shape> {
    let [first, second] = items else {
        return None;
    };
    let read_at = |at: usize, value: &Value| {
        node.child(value, &[key, &at.to_string()])
            .and_then(read)
            .ok()
    };
    let (first, second) = (read_at(0, first)?, read_at(1, second)?);
    let handle = Shape::handle();
    match (first == handle, second == handle) {
        (false, true) => Some(Shape::OrHandle(Box::new(first))),
        (true, false) => Some(Shape::OrHandle(Box::new(second))),
        _ => None,
    }
}
