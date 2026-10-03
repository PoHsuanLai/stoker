//! The tagged enum: `oneOf` (or the strict dialect's `anyOf`) of `{tag, content}` objects.

use serde_json::Value;

use super::{ANNOTATIONS, Found, Node, Refused, read};
use crate::{FieldName, Shape, Variant, VariantName};

/// One property of a tagged arm: its name, the one string it allows (if it is a one-string
/// enum) and its schema.
#[derive(Clone, Copy)]
struct Prop<'a> {
    name: &'a str,
    only: Option<&'a str>,
    schema: &'a Value,
}

/// The one string an `{"type":"string","enum":[x]}` schema allows.
fn sole_string(schema: &Value) -> Option<&str> {
    let map = schema.as_object()?;
    let only = match map.get("enum")?.as_array()?.as_slice() {
        [Value::String(s)] => s.as_str(),
        _ => return None,
    };
    map.keys()
        .all(|k| matches!(k.as_str(), "type" | "enum") || ANNOTATIONS.contains(&k.as_str()))
        .then_some(only)
}

/// The two properties of `{tag, content}` object, in name order.
fn arm(item: &Value) -> Option<[Prop<'_>; 2]> {
    let map = item.as_object()?;
    if map.get("additionalProperties") != Some(&Value::Bool(false)) {
        return None;
    }
    let properties = map.get("properties")?.as_object()?;
    let props: Vec<Prop<'_>> = properties
        .iter()
        .map(|(name, schema)| Prop {
            name,
            only: sole_string(schema),
            schema,
        })
        .collect();
    let required = map.get("required")?.as_array()?;
    let all_required = props
        .iter()
        .all(|p| required.iter().any(|r| r.as_str() == Some(p.name)));
    match (props[..].try_into(), required.len()) {
        (Ok(pair), 2) if all_required => Some(pair),
        _ => None,
    }
}

/// The names the arms use for their tag and content, when every arm uses the same two.
fn shared_names<'a>(arms: &[[Prop<'a>; 2]]) -> Option<[&'a str; 2]> {
    let names = arms.first()?.map(|p| p.name);
    arms.iter()
        .all(|a| a.map(|p| p.name) == names)
        .then_some(names)
}

/// Which of the two properties is the tag: the one that is a one-string enum in every arm with
/// strings that differ from arm to arm. Both qualifying (or neither) is ambiguous.
fn tag_position(arms: &[[Prop<'_>; 2]]) -> Option<usize> {
    let qualifies = |at: usize| {
        let strings: Option<Vec<&str>> = arms.iter().map(|a| a[at].only).collect();
        strings.is_some_and(|s| {
            s.iter()
                .enumerate()
                .all(|(i, a)| s[i + 1..].iter().all(|b| a != b))
        })
    };
    match (qualifies(0), qualifies(1)) {
        (true, false) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

pub(super) fn tagged(node: &mut Node, key: &str, items: &[Value]) -> Found<Shape> {
    let bad = |node: &Node| node.refuse(Refused::BadTagged);
    let arms: Vec<[Prop<'_>; 2]> = items
        .iter()
        .map(arm)
        .collect::<Option<_>>()
        .ok_or_else(|| bad(node))?;
    let (Some(names), Some(tag_at)) = (shared_names(&arms), tag_position(&arms)) else {
        return Err(bad(node));
    };
    let (tag, content) = (names[tag_at], names[1 - tag_at]);
    let variants = arms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let at = [key, &i.to_string(), "properties", content];
            let shape = read(node.child(a[1 - tag_at].schema, &at)?)?;
            let name = a[tag_at].only.map(VariantName::new);
            let name = name.and_then(Result::ok).ok_or_else(|| bad(node))?;
            Ok(Variant { name, shape })
        })
        .collect::<Found<Vec<_>>>()?;
    Ok(Shape::Tagged {
        tag: FieldName::new(tag).map_err(|_| bad(node))?,
        content: FieldName::new(content).map_err(|_| bad(node))?,
        variants,
    })
}
