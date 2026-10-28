//! The JSON body of `POST /chat/completions`.
//!
//! Every field a flavor does not understand is left out, never sent hopefully: a server that
//! ignores an unknown field silently drops the constraint it was meant to carry. Parameter
//! spellings were checked against the engines' current docs on 2026-10-03 (see `FINDINGS.md`);
//! the rows marked "to verify" there wait for a recorded request.

use model_provider::{
    Constraint, Knob, Limits, Message, Milli, OutputShape, Part, Reasoning, Sampling,
    ShapeWithTools, ToolChoice, ToolParallelism, ToolSpec, TurnRequest,
};
use model_wire::CodecError;
use serde_json::{Map, Value, json};

use crate::{Flavor, RequestJson};
use crate::{logprobs, messages};

/// The JSON body of `POST /chat/completions` for `request`, streaming on.
pub fn encode_request(request: &TurnRequest, flavor: Flavor) -> Result<RequestJson, CodecError> {
    let server = flavor.server();
    let mut body = Map::new();
    body.insert("model".into(), json!(request.model.0));
    body.insert(
        "messages".into(),
        Value::Array(messages::encode(
            &request.messages,
            server.images_in_tool_message(),
        )?),
    );
    body.insert("stream".into(), json!(true));
    if server.asks_usage() {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    tools(&mut body, request, server.honours_named_tool())?;
    limits(&mut body, &request.limits);
    sampling(&mut body, &request.sampling, flavor);
    reasoning(&mut body, request.reasoning, flavor);
    shape(&mut body, request, flavor)?;
    logprobs::encode(&mut body, request.choice_scores, flavor);
    server.extras(&mut body, &request.engine);
    Ok(RequestJson(Value::Object(body).to_string()))
}

/// Thousandths as the decimal a server reads.
fn milli(value: Milli) -> Value {
    json!(f64::from(value.0) / 1000.0)
}

fn tools(
    body: &mut Map<String, Value>,
    request: &TurnRequest,
    honours_named: bool,
) -> Result<(), CodecError> {
    if !honours_named && matches!(request.tool_choice, ToolChoice::Named(_)) {
        return Err(CodecError::UnsupportedShape);
    }
    if request.tools.is_empty() {
        return Ok(());
    }
    let specs = request
        .tools
        .iter()
        .map(|spec| match spec {
            ToolSpec::Function {
                name,
                description,
                parameters,
            } => {
                let schema: Value = serde_json::from_str(parameters.0.as_str())
                    .map_err(|_| CodecError::UnsupportedShape)?;
                Ok(json!({
                    "type": "function",
                    "function": {
                        "name": name.as_str(),
                        "description": description,
                        "parameters": schema,
                    },
                }))
            }
            ToolSpec::Native(_) => Err(CodecError::NativeToolUnsupported),
            _ => Err(CodecError::UnsupportedShape),
        })
        .collect::<Result<Vec<_>, _>>()?;
    body.insert("tools".into(), Value::Array(specs));
    let choice = match &request.tool_choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::Never => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Named(name) => {
            json!({ "type": "function", "function": { "name": name.as_str() } })
        }
        // A choice this build does not know cannot be sent faithfully: refuse the request.
        _ => return Err(CodecError::UnsupportedShape),
    };
    body.insert("tool_choice".into(), choice);
    body.insert(
        "parallel_tool_calls".into(),
        json!(request.tool_calls == ToolParallelism::Many),
    );
    Ok(())
}

fn limits(body: &mut Map<String, Value>, limits: &Limits) {
    body.insert("max_tokens".into(), json!(limits.max_output.0));
    if !limits.stop.is_empty() {
        body.insert("stop".into(), json!(limits.stop));
    }
}

fn sampling(body: &mut Map<String, Value>, sampling: &Sampling, flavor: Flavor) {
    body.insert("temperature".into(), milli(sampling.temperature));
    if let Knob::Set(top_p) = sampling.top_p {
        body.insert("top_p".into(), milli(top_p));
    }
    if let Knob::Set(top_k) = sampling.top_k {
        body.insert("top_k".into(), json!(top_k.0));
    }
    if let Knob::Set(min_p) = sampling.min_p {
        body.insert("min_p".into(), milli(min_p));
    }
    if let Knob::Set(penalty) = sampling.repeat_penalty {
        body.insert(flavor.server().repeat_penalty_key().into(), milli(penalty));
    }
    if let Knob::Set(seed) = sampling.seed {
        body.insert("seed".into(), json!(seed.0));
    }
}

/// Reasoning on or off, where the flavor has a switch: the chat template's `enable_thinking`
/// (llama-server, vLLM), `reasoning_effort` (vLLM, LiteLLM) or OpenRouter's `reasoning` object.
/// `EngineDefault` sends none of them.
fn reasoning(body: &mut Map<String, Value>, reasoning: Reasoning, flavor: Flavor) {
    let effort = match reasoning {
        Reasoning::EngineDefault => return,
        Reasoning::On(effort) => Some(
            serde_json::to_value(effort)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
        ),
        Reasoning::Off => None,
        // A mode this build does not know sends nothing, like the engine default.
        _ => return,
    };
    flavor.server().reasoning(body, effort.as_deref());
}

/// Whether a tool result is already in the conversation.
fn has_tool_result(messages: &[Message]) -> bool {
    messages
        .iter()
        .flat_map(|m| &m.parts)
        .any(|p| matches!(p, Part::ToolResult(_)))
}

/// The output constraint, in the flavor's spelling, subject to what the flavor does with tools.
fn shape(
    body: &mut Map<String, Value>,
    request: &TurnRequest,
    flavor: Flavor,
) -> Result<(), CodecError> {
    if request.output == OutputShape::Free {
        return Ok(());
    }
    if !request.tools.is_empty() {
        match flavor.shape_with_tools() {
            ShapeWithTools::Together => {}
            ShapeWithTools::AfterResult if has_tool_result(&request.messages) => {}
            ShapeWithTools::AfterResult => return Ok(()),
            ShapeWithTools::Refuse => return Err(CodecError::UnsupportedShape),
        }
    }
    flavor.server().shape(body, &request.output)
}

/// The constraint kinds a flavor can enforce on a request: what `Caps.output` of a model behind
/// it may hold.
pub fn enforceable(flavor: Flavor) -> Vec<Constraint> {
    flavor.server().enforceable().to_vec()
}
