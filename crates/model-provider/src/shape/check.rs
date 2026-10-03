//! The checker for the shape vocabulary: our own, so no `jsonschema` crate.
//!
//! A fault names where (the nearest enclosing field, `root` at the top) and what kind, and never
//! a value: the reply may hold untrusted text. An unknown key is reported against the field that
//! holds it for the same reason, because the key is the model's text.

use serde_json::{Map, Value};

use crate::{FieldName, JsonText, Shape, ShapeFault, ShapeKind};

impl Shape {
    pub(crate) fn check_json(&self, json: &JsonText) -> Result<(), ShapeFault> {
        let value: Value = serde_json::from_str(json.as_str()).map_err(|_| ShapeFault::NotJson)?;
        self.check_value(&value, &root())
    }

    fn check_value(&self, value: &Value, at: &FieldName) -> Result<(), ShapeFault> {
        let mismatch = |want| ShapeFault::Mismatch {
            at: at.clone(),
            want,
        };
        match (self, value) {
            (Shape::Optional(_), Value::Null) => Ok(()),
            (Shape::Optional(inner), _) => inner.check_value(value, at),
            (Shape::OrHandle(_), Value::Object(map)) if is_handle(map) => Ok(()),
            (Shape::OrHandle(inner), _) => inner.check_value(value, at),
            (Shape::Choice(choices), Value::String(text)) => choices
                .iter()
                .any(|c| &c.0 == text)
                .then_some(())
                .ok_or_else(|| mismatch(ShapeKind::Choice)),
            (Shape::Integer { min, max }, Value::Number(n)) => n
                .as_i64()
                .filter(|n| (*min..=*max).contains(n))
                .map(drop)
                .ok_or_else(|| mismatch(ShapeKind::Integer)),
            (Shape::Text { max }, Value::String(text)) => (text.chars().count() <= max.0 as usize)
                .then_some(())
                .ok_or_else(|| mismatch(ShapeKind::Text)),
            (Shape::Date, Value::String(text)) => valid_date(text)
                .then_some(())
                .ok_or_else(|| mismatch(ShapeKind::Date)),
            (Shape::DateTime, Value::String(text)) => valid_date_time(text)
                .then_some(())
                .ok_or_else(|| mismatch(ShapeKind::DateTime)),
            (Shape::List { of, max }, Value::Array(items)) => {
                if items.len() > max.0 as usize {
                    return Err(mismatch(ShapeKind::List));
                }
                items.iter().try_for_each(|item| of.check_value(item, at))
            }
            (Shape::Record(fields), Value::Object(map)) => check_record(fields, map, at),
            (
                Shape::Tagged {
                    tag,
                    content,
                    variants,
                },
                Value::Object(map),
            ) => check_tagged(tag, content, variants, map, at),
            _ => Err(mismatch(self.kind())),
        }
    }

    fn kind(&self) -> ShapeKind {
        match self {
            Shape::Choice(_) => ShapeKind::Choice,
            Shape::Integer { .. } => ShapeKind::Integer,
            Shape::Text { .. } => ShapeKind::Text,
            Shape::Date => ShapeKind::Date,
            Shape::DateTime => ShapeKind::DateTime,
            Shape::Record(_) => ShapeKind::Record,
            Shape::List { .. } => ShapeKind::List,
            Shape::Optional(inner) | Shape::OrHandle(inner) => inner.kind(),
            Shape::Tagged { .. } => ShapeKind::Tagged,
        }
    }
}

/// `{ "handle": n }` with `n` a whole number from 0 and nothing else in the object.
fn is_handle(map: &Map<String, Value>) -> bool {
    map.len() == 1
        && map
            .get("handle")
            .and_then(Value::as_i64)
            .is_some_and(|n| n >= 0)
}

fn root() -> FieldName {
    FieldName::new("root").unwrap_or_else(|_| unreachable!("`root` is a valid name"))
}

fn check_record(
    fields: &[crate::Field],
    map: &Map<String, Value>,
    at: &FieldName,
) -> Result<(), ShapeFault> {
    if map
        .keys()
        .any(|key| !fields.iter().any(|f| f.name.as_str() == key))
    {
        return Err(ShapeFault::Unknown { field: at.clone() });
    }
    fields
        .iter()
        .try_for_each(|field| match map.get(field.name.as_str()) {
            Some(value) => field.shape.check_value(value, &field.name),
            None if matches!(field.shape, Shape::Optional(_)) => Ok(()),
            None => Err(ShapeFault::Missing {
                field: field.name.clone(),
            }),
        })
}

fn check_tagged(
    tag: &FieldName,
    content: &FieldName,
    variants: &[crate::Variant],
    map: &Map<String, Value>,
    at: &FieldName,
) -> Result<(), ShapeFault> {
    if map
        .keys()
        .any(|k| k != tag.as_str() && k != content.as_str())
    {
        return Err(ShapeFault::Unknown { field: at.clone() });
    }
    let chosen = map
        .get(tag.as_str())
        .ok_or_else(|| ShapeFault::Missing { field: tag.clone() })?;
    let variant = chosen
        .as_str()
        .and_then(|name| variants.iter().find(|v| v.name.as_str() == name))
        .ok_or_else(|| ShapeFault::Mismatch {
            at: tag.clone(),
            want: ShapeKind::Tagged,
        })?;
    let body = map
        .get(content.as_str())
        .ok_or_else(|| ShapeFault::Missing {
            field: content.clone(),
        })?;
    variant.shape.check_value(body, content)
}

fn number(text: &str, width: usize) -> Option<u32> {
    (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn valid_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts[..] else {
        return false;
    };
    let (Some(year), Some(month), Some(day)) = (number(year, 4), number(month, 2), number(day, 2))
    else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// RFC 3339: `YYYY-MM-DDTHH:MM:SS`, optional fraction, then `Z` or `+HH:MM` / `-HH:MM`.
fn valid_date_time(text: &str) -> bool {
    let Some((date, time)) = text.split_once('T') else {
        return false;
    };
    let (clock, zone) = match time.strip_suffix('Z') {
        Some(clock) => (clock, None),
        None => match time.rfind(['+', '-']) {
            Some(i) => (&time[..i], Some(&time[i + 1..])),
            None => return false,
        },
    };
    let (whole, fraction) = clock
        .split_once('.')
        .map_or((clock, None), |(w, f)| (w, Some(f)));
    let clock_ok = match whole.split(':').collect::<Vec<_>>()[..] {
        [h, m, s] => matches!(
            (number(h, 2), number(m, 2), number(s, 2)),
            (Some(h), Some(m), Some(s)) if h < 24 && m < 60 && s < 60
        ),
        _ => false,
    };
    let fraction_ok =
        fraction.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()));
    let zone_ok = zone.is_none_or(|z| match z.split(':').collect::<Vec<_>>()[..] {
        [h, m] => matches!((number(h, 2), number(m, 2)), (Some(h), Some(m)) if h < 24 && m < 60),
        _ => false,
    });
    valid_date(date) && clock_ok && fraction_ok && zone_ok
}
