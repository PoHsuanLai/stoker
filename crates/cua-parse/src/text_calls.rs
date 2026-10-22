//! Tool calls read from the reply text, for a model whose server does not parse them.
//!
//! A model fine-tuned on its chat template's call format writes `<tool_call>` blocks, and a
//! server-side tool parser (vLLM's `--tool-call-parser`) would strip them from the text, which
//! breaks a model that also reasons in that format. Such a catalogue entry says its calls are read
//! by us (`cua_action::CallFormat`); this module turns the text into the same `ToolCall` values
//! the server path produces, so profile aliases and verb handling run unchanged after it.
//!
//! The reading is strict and total. Only `<tool_call>` blocks count; a call in any other form (an
//! `<invoke>` block, a bare JSON object) is not read, and the reply is `NoAction`. The text after
//! the last `</think>` is the answer, and its calls are the step's. A reply that wrote its call
//! inside the reasoning and nothing after it still acts: the last block of the whole text counts.
//! The scanned size and the number of calls are bounded by [`ParseLimits`].

use std::collections::BTreeMap;

use cua_action::CallFormat;
use model_provider::{JsonText, ToolCall, ToolCallId, ToolName};
use serde_json::{Map, Value};

use crate::{ParseError, ParseLimits};

const OPEN: &str = "<tool_call>";
const CLOSE: &str = "</tool_call>";
const THINK_CLOSE: &str = "</think>";
const FUNCTION: &str = "<function=";
const PARAMETER: &str = "<parameter=";
const PARAMETER_CLOSE: &str = "</parameter>";

/// How a parameter's text is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Always a string: typed text that looks like a number stays text.
    Text,
    /// A JSON number when it reads as one.
    Number,
    /// An array, an object or a boolean when it reads as JSON.
    Structured,
}

/// The kind of each parameter of a tool, taken from its JSON schema: what a server's parser knows
/// and a plain text reading does not. A parameter the schema does not name is read as structured
/// when its text is a JSON array or object, else as a string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParamKinds(BTreeMap<String, Kind>);

impl ParamKinds {
    /// The kinds of `schema`'s `properties`; a schema that is not JSON names none.
    pub fn from_schema(schema: &str) -> ParamKinds {
        let kinds = serde_json::from_str::<Value>(schema)
            .ok()
            .and_then(|doc| match doc.get("properties") {
                Some(Value::Object(properties)) => Some(properties.clone()),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(name, property)| (name, kind_of(property.get("type"))))
            .collect();
        ParamKinds(kinds)
    }

    fn value(&self, name: &str, raw: &str) -> Value {
        let text = || Value::String(raw.to_owned());
        let json = || serde_json::from_str::<Value>(raw).ok();
        match self.0.get(name) {
            Some(Kind::Text) => text(),
            Some(Kind::Number) => json().filter(Value::is_number).unwrap_or_else(text),
            Some(Kind::Structured) => json().unwrap_or_else(text),
            None if raw.starts_with(['[', '{']) => json().unwrap_or_else(text),
            None => text(),
        }
    }
}

fn kind_of(declared: Option<&Value>) -> Kind {
    let says = |word: &str| match declared {
        Some(Value::String(t)) => t == word,
        Some(Value::Array(list)) => list.iter().any(|t| t.as_str() == Some(word)),
        _ => false,
    };
    if says("string") {
        Kind::Text
    } else if says("integer") || says("number") {
        Kind::Number
    } else {
        Kind::Structured
    }
}

/// The calls a reply makes, read from `answer` (the reply text) and `thought` (the reasoning
/// text, when the server separated it).
///
/// The calls after the last `</think>` of the answer are the step's, at most `limits.max_actions`
/// of them. When there are none, the last call written anywhere in the thought or the answer is.
/// No call is `NoAction`; the two texts together over `limits.max_input` bytes are `TooLarge`.
pub fn read_calls(
    format: CallFormat,
    answer: &str,
    thought: &str,
    kinds: &ParamKinds,
    limits: ParseLimits,
) -> Result<Vec<ToolCall>, ParseError> {
    if answer.len().saturating_add(thought.len()) > limits.max_input.0 as usize {
        return Err(ParseError::TooLarge);
    }
    let after = answer
        .rsplit_once(THINK_CLOSE)
        .map_or(answer, |(_, tail)| tail);
    let read = |text: &str| {
        blocks(text)
            .enumerate()
            .filter_map(|(n, block)| call(format, block, kinds, n))
            .collect::<Vec<ToolCall>>()
    };
    let mut calls = read(after);
    calls.truncate(usize::from(limits.max_actions.0));
    if calls.is_empty() {
        calls.extend(read(thought).into_iter().chain(read(answer)).next_back());
    }
    if calls.is_empty() {
        Err(ParseError::NoAction)
    } else {
        Ok(calls)
    }
}

/// The inside of each `<tool_call>` block, in order. A block that never closes is not one. When a
/// block opens again before it closes, the inner opening is the real one.
fn blocks(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let (_, tail) = rest.split_once(OPEN)?;
        let (block, after) = tail.split_once(CLOSE)?;
        rest = after;
        Some(block.rsplit_once(OPEN).map_or(block, |(_, inner)| inner))
    })
}

/// One block as a call, or `None` for a block that is not well formed in `format`.
fn call(format: CallFormat, block: &str, kinds: &ParamKinds, n: usize) -> Option<ToolCall> {
    let (name, args) = match format {
        CallFormat::Qwen3CoderXml => qwen_xml(block, kinds)?,
        CallFormat::HermesJson => hermes_json(block)?,
        CallFormat::Engine => return None,
    };
    Some(ToolCall {
        id: ToolCallId(format!("text-{n}")),
        name: ToolName::new(name).ok()?,
        input: JsonText::from_value(&Value::Object(args)),
    })
}

/// `<function=NAME>` and its `<parameter=KEY>VALUE</parameter>` lines. A parameter that never
/// closes ends the reading; what was read before it stays.
fn qwen_xml(block: &str, kinds: &ParamKinds) -> Option<(String, Map<String, Value>)> {
    let (_, after) = block.split_once(FUNCTION)?;
    let (name, mut rest) = after.split_once('>')?;
    let mut args = Map::new();
    while let Some((_, tail)) = rest.split_once(PARAMETER) {
        let Some((key, tail)) = tail.split_once('>') else {
            break;
        };
        let Some((value, tail)) = tail.split_once(PARAMETER_CLOSE) else {
            break;
        };
        let key = key.trim();
        args.insert(key.to_owned(), kinds.value(key, value.trim()));
        rest = tail;
    }
    Some((name.trim().to_owned(), args))
}

/// `{"name": .., "arguments": {..}}`; the arguments may also be a string holding an object.
fn hermes_json(block: &str) -> Option<(String, Map<String, Value>)> {
    let Value::Object(mut doc) = serde_json::from_str(block.trim()).ok()? else {
        return None;
    };
    let Some(Value::String(name)) = doc.remove("name") else {
        return None;
    };
    let args = match doc.remove("arguments") {
        None => Map::new(),
        Some(Value::Object(args)) => args,
        Some(Value::String(text)) => match serde_json::from_str(&text).ok()? {
            Value::Object(args) => args,
            _ => return None,
        },
        Some(_) => return None,
    };
    Some((name, args))
}
