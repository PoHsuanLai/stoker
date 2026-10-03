//! Embeddings: `POST /embeddings`, and the `data[].embedding` of its reply.

use model_http::{Exchange, Framing, JsonBody, RouteRoot, UrlPath, Verb};
use model_provider::{EmbedEnd, EmbedTurn, EmbedVector, Knob, ModelName, Tokens, TurnUsage};
use model_wire::CodecError;
use serde_json::{Map, Value, json};

use crate::{DimensionsField, Flavor};

/// The request: the model, the inputs (the role's prefix is already on them), `float` encoding,
/// and `dimensions` only where the server honours it (llama.cpp ignores it, and a width it never
/// produced would make the reported width a lie).
pub(crate) fn encode(flavor: Flavor, turn: &EmbedTurn) -> Exchange {
    let mut body = Map::new();
    body.insert("model".into(), json!(turn.model.0));
    body.insert("input".into(), json!(turn.inputs));
    body.insert("encoding_format".into(), json!("float"));
    if let (Knob::Set(dims), DimensionsField::Send) = (turn.dims, flavor.quirks().dimensions) {
        body.insert("dimensions".into(), json!(dims.0));
    }
    Exchange {
        verb: Verb::PostJson,
        root: RouteRoot::Base,
        path: UrlPath("/embeddings".into()),
        body: Some(JsonBody(Value::Object(body).to_string())),
        framing: Framing::Whole,
    }
}

/// The vectors in `index` order (the reply's position when it gives none). A hole, a repeated
/// index, a non-numeric element or a missing `data` is unreadable.
pub(crate) fn decode(served: ModelName, body: &[u8]) -> Result<EmbedEnd, CodecError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| CodecError::Unreadable)?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(CodecError::Unreadable)?;
    let mut slots: Vec<Option<EmbedVector>> = vec![None; data.len()];
    for (position, item) in data.iter().enumerate() {
        let index = match item.get("index") {
            None | Some(Value::Null) => position,
            Some(n) => n
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(CodecError::Unreadable)?,
        };
        let slot = slots.get_mut(index).ok_or(CodecError::Unreadable)?;
        if slot.is_some() {
            return Err(CodecError::Unreadable);
        }
        *slot = Some(vector(item)?);
    }
    let vectors = slots
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(CodecError::Unreadable)?;
    let tokens = |key: &str| {
        value
            .get("usage")
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .map_or(Tokens(0), |n| Tokens(u32::try_from(n).unwrap_or(u32::MAX)))
    };
    Ok(EmbedEnd {
        vectors,
        usage: TurnUsage {
            input: tokens("prompt_tokens"),
            ..TurnUsage::default()
        },
        served,
    })
}

fn vector(item: &Value) -> Result<EmbedVector, CodecError> {
    item.get("embedding")
        .and_then(Value::as_array)
        .ok_or(CodecError::Unreadable)?
        .iter()
        .map(|n| n.as_f64().map(|f| f as f32).ok_or(CodecError::Unreadable))
        .collect::<Result<Vec<f32>, _>>()
        .map(EmbedVector)
}
