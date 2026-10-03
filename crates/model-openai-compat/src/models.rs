//! `describe`: which request lists the models, and how its body reads.

use model_http::{Exchange, Framing, RouteRoot, UrlPath, Verb};
use model_provider::{ModelInfo, ModelName, Tokens};
use model_wire::CodecError;
use serde_json::Value;

use crate::Flavor;

/// `GET /models` under the base, or `/props` at the server root for llama-server (its models list
/// carries no loaded context).
pub(crate) fn describe(flavor: Flavor) -> Exchange {
    let root = flavor.quirks().describe_root;
    let path = match root {
        RouteRoot::Server => "/props",
        RouteRoot::Base => "/models",
    };
    Exchange {
        verb: Verb::Get,
        root,
        path: UrlPath(path.into()),
        body: None,
        framing: Framing::Whole,
    }
}

/// The models a body lists: `{"data": [...]}` (vLLM `max_model_len`, OpenRouter
/// `context_length`, llama-server `meta.n_ctx_train`) or llama-server's `/props` object (the
/// per-slot `n_ctx` it loaded). A size the server does not report is `Tokens(0)`: the planner
/// then uses the catalog's context for that model. When only one of the two sizes is reported
/// both take it.
pub(crate) fn parse_models(body: &[u8]) -> Result<Vec<ModelInfo>, CodecError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| CodecError::Unreadable)?;
    match value.get("data").and_then(Value::as_array) {
        Some(entries) => entries.iter().map(entry).collect(),
        None => props(&value).map(|info| vec![info]),
    }
}

fn size(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
}

fn info(name: String, loaded: Option<u32>, trained: Option<u32>) -> ModelInfo {
    let (loaded, trained) = match (loaded, trained) {
        (Some(l), Some(t)) => (l, t),
        (Some(n), None) | (None, Some(n)) => (n, n),
        (None, None) => (0, 0),
    };
    ModelInfo {
        name: ModelName(name),
        loaded_context: Tokens(loaded),
        trained_context: Tokens(trained),
    }
}

fn entry(entry: &Value) -> Result<ModelInfo, CodecError> {
    let name = entry
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(CodecError::Unreadable)?;
    let meta = entry.get("meta");
    let loaded = size(entry.get("max_model_len")).or_else(|| size(entry.get("context_length")));
    let trained = size(meta.and_then(|m| m.get("n_ctx_train")));
    Ok(info(name.to_owned(), loaded, trained))
}

fn props(value: &Value) -> Result<ModelInfo, CodecError> {
    let settings = value
        .get("default_generation_settings")
        .ok_or(CodecError::Unreadable)?;
    let alias = value.get("model_alias").and_then(Value::as_str);
    let path = value.get("model_path").and_then(Value::as_str);
    let name = alias
        .filter(|a| !a.is_empty())
        .or_else(|| path.and_then(|p| p.rsplit(['/', '\\']).next()))
        .filter(|n| !n.is_empty())
        .ok_or(CodecError::Unreadable)?;
    Ok(info(name.to_owned(), size(settings.get("n_ctx")), None))
}
