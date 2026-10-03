//! The JSON body of `POST /chat/completions`.
//!
//! Every field a flavor does not understand is left out, never sent hopefully: a server that
//! ignores an unknown field silently drops the constraint it was meant to carry. Parameter
//! spellings were checked against the engines' current docs on 2026-10-03 (see `FINDINGS.md`);
//! the rows marked "to verify" there wait for a recorded request.

use model_provider::{
    Constraint, EngineExtras, Knob, Limits, Message, Milli, OutputShape, Part, PromptCache,
    Reasoning, Sampling, ShapeWithTools, ToolChoice, ToolParallelism, ToolSpec, TurnRequest,
};
use model_wire::CodecError;
use serde_json::{Map, Value, json};

use crate::messages;
use crate::{Flavor, RequestJson, ToolNaming, UsageAsk};

/// The JSON body of `POST /chat/completions` for `request`, streaming on.
pub fn encode_request(request: &TurnRequest, flavor: Flavor) -> Result<RequestJson, CodecError> {
    let quirks = flavor.quirks();
    let mut body = Map::new();
    body.insert("model".into(), json!(request.model.0));
    body.insert(
        "messages".into(),
        Value::Array(messages::encode(&request.messages, quirks.tool_images)?),
    );
    body.insert("stream".into(), json!(true));
    if quirks.usage == UsageAsk::Request {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    tools(&mut body, request, quirks.tool_naming)?;
    limits(&mut body, &request.limits);
    sampling(&mut body, &request.sampling, flavor);
    reasoning(&mut body, request.reasoning, flavor);
    shape(&mut body, request, flavor)?;
    extras(&mut body, &request.engine, flavor);
    Ok(RequestJson(Value::Object(body).to_string()))
}

/// Thousandths as the decimal a server reads.
fn milli(value: Milli) -> Value {
    json!(f64::from(value.0) / 1000.0)
}

fn tools(
    body: &mut Map<String, Value>,
    request: &TurnRequest,
    naming: ToolNaming,
) -> Result<(), CodecError> {
    if naming == ToolNaming::AutoOnly && matches!(request.tool_choice, ToolChoice::Named(_)) {
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
        let name = match flavor {
            Flavor::LlamaServer => "repeat_penalty",
            Flavor::Vllm | Flavor::LiteLlm | Flavor::OpenRouter => "repetition_penalty",
        };
        body.insert(name.into(), milli(penalty));
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
    };
    match (flavor, effort) {
        (Flavor::LlamaServer, effort) => {
            body.insert(
                "chat_template_kwargs".into(),
                json!({ "enable_thinking": effort.is_some() }),
            );
        }
        (Flavor::Vllm, effort) => {
            body.insert(
                "chat_template_kwargs".into(),
                json!({ "enable_thinking": effort.is_some() }),
            );
            if let Some(effort) = effort {
                body.insert("reasoning_effort".into(), json!(effort));
            }
        }
        (Flavor::LiteLlm, Some(effort)) => {
            body.insert("reasoning_effort".into(), json!(effort));
        }
        (Flavor::OpenRouter, Some(effort)) => {
            body.insert("reasoning".into(), json!({ "effort": effort }));
        }
        (Flavor::LiteLlm | Flavor::OpenRouter, None) => {}
    }
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
        match flavor.quirks().shape_with_tools {
            ShapeWithTools::Together => {}
            ShapeWithTools::AfterResult if has_tool_result(&request.messages) => {}
            ShapeWithTools::AfterResult => return Ok(()),
            ShapeWithTools::Refuse => return Err(CodecError::UnsupportedShape),
        }
    }
    match (&request.output, flavor) {
        (OutputShape::Free, _) => {}
        (OutputShape::JsonSchema(schema), _) => {
            let schema: Value = serde_json::from_str(schema.0.as_str())
                .map_err(|_| CodecError::UnsupportedShape)?;
            body.insert(
                "response_format".into(),
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": "reply", "strict": true, "schema": schema },
                }),
            );
        }
        (OutputShape::Gbnf(grammar), Flavor::LlamaServer) => {
            body.insert("grammar".into(), json!(grammar));
        }
        (OutputShape::Choice(choices), Flavor::LlamaServer) => {
            body.insert("grammar".into(), json!(choice_grammar(choices)));
        }
        (OutputShape::Regex(regex), Flavor::Vllm) => {
            body.insert("structured_outputs".into(), json!({ "regex": regex }));
        }
        (OutputShape::Lark(grammar), Flavor::Vllm) => {
            body.insert("structured_outputs".into(), json!({ "grammar": grammar }));
        }
        (OutputShape::Choice(choices), Flavor::Vllm) => {
            body.insert("structured_outputs".into(), json!({ "choice": choices }));
        }
        _ => return Err(CodecError::UnsupportedShape),
    }
    Ok(())
}

/// `root ::= "a" | "b"`: a reply that is exactly one of the strings, in llama.cpp's grammar.
fn choice_grammar(choices: &[String]) -> String {
    let quoted: Vec<String> = choices.iter().map(|c| gbnf_literal(c)).collect();
    format!("root ::= {}", quoted.join(" | "))
}

fn gbnf_literal(text: &str) -> String {
    let body: String = text
        .chars()
        .map(|c| match c {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            '\t' => "\\t".to_owned(),
            c if c.is_control() => format!("\\x{:02X}", u32::from(c)),
            c => c.to_string(),
        })
        .collect();
    format!("\"{body}\"")
}

/// What only this flavor understands; the other flavors' arms are ignored.
fn extras(body: &mut Map<String, Value>, engine: &EngineExtras, flavor: Flavor) {
    match (engine, flavor) {
        (EngineExtras::LlamaServer(extras), Flavor::LlamaServer) => {
            body.insert(
                "cache_prompt".into(),
                json!(extras.cache_prompt == PromptCache::Reuse),
            );
            if let Knob::Set(slot) = extras.slot {
                body.insert("id_slot".into(), json!(slot.0));
            }
        }
        (EngineExtras::Vllm(extras), Flavor::Vllm) => {
            if let Knob::Set(priority) = extras.priority {
                body.insert("priority".into(), json!(priority.0));
            }
        }
        _ => {}
    }
}

/// The constraint kinds a flavor can enforce on a request: what `Caps.output` of a model behind
/// it may hold.
pub fn enforceable(flavor: Flavor) -> Vec<Constraint> {
    match flavor {
        Flavor::LlamaServer => vec![Constraint::JsonSchema, Constraint::Gbnf, Constraint::Choice],
        Flavor::Vllm => vec![
            Constraint::JsonSchema,
            Constraint::Regex,
            Constraint::Lark,
            Constraint::Choice,
        ],
        Flavor::LiteLlm | Flavor::OpenRouter => vec![Constraint::JsonSchema],
    }
}
