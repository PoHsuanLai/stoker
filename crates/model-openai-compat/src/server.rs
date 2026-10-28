//! What differs between the servers that share the chat-completions wire.
//!
//! A [`Server`] has a default method for the common case, and each server overrides only where
//! it differs (the shape of `Provider::charges`). The rows come from rig's dialect constants and
//! our own survey; entries marked "to verify" are pinned by the first recorded fixture of that
//! engine (`FINDINGS.md`). [`Flavor`] names the server at run time and picks its type.

use model_http::RouteRoot;
use model_provider::{
    Charges, Constraint, EngineExtras, Knob, OutputShape, PromptCache, ShapeWithTools,
};
use model_wire::CodecError;
use serde_json::{Map, Value, json};

use crate::Flavor;

impl Flavor {
    /// How a requested output shape combines with tools on this server.
    pub fn shape_with_tools(self) -> ShapeWithTools {
        self.server().shape_with_tools()
    }

    /// The server's behaviour.
    pub(crate) fn server(self) -> &'static dyn Server {
        match self {
            Flavor::LlamaServer => &LlamaServer,
            Flavor::Vllm => &Vllm,
            Flavor::LiteLlm => &LiteLlm,
            Flavor::OpenRouter => &OpenRouter,
        }
    }
}

/// One server's differences on the shared wire. The defaults are the common case.
pub(crate) trait Server {
    /// Whether the request asks for a usage chunk (`stream_options.include_usage`). OpenRouter
    /// always sends usage and rejects the option.
    fn asks_usage(&self) -> bool {
        true
    }

    /// Whether a named `tool_choice` is honoured. llama-server treats one as `auto`, so a request
    /// for one is refused at encode.
    fn honours_named_tool(&self) -> bool {
        true
    }

    /// Whether an image in a tool result stays inside the `role: "tool"` message; otherwise it
    /// moves to the user message that follows.
    fn images_in_tool_message(&self) -> bool {
        false
    }

    // To verify: whether `response_format` suppresses tool calls on llama-server.
    fn shape_with_tools(&self) -> ShapeWithTools {
        ShapeWithTools::AfterResult
    }

    /// Where `describe` reads.
    fn describe_root(&self) -> RouteRoot {
        RouteRoot::Base
    }

    /// Whether the `dimensions` field of an embeddings request is honoured. llama.cpp ignores
    /// it, and a width it never produced would make the reported width a lie.
    fn honours_dimensions(&self) -> bool {
        true
    }

    /// Whether `logprobs`/`top_logprobs` are sent for a first-token ask. Hosted gateways: not
    /// asked, so no reply carries them.
    fn asks_logprobs(&self) -> bool {
        false
    }

    fn charges(&self) -> Charges {
        Charges::NotReported
    }

    /// Whether cached input is read from llama-server's `timings.cache_n` before the
    /// `prompt_tokens_details.cached_tokens` of the others.
    fn cache_from_timings_first(&self) -> bool {
        false
    }

    fn repeat_penalty_key(&self) -> &'static str {
        "repetition_penalty"
    }

    /// Reasoning on (`Some(effort)`) or off (`None`), where the server has a switch.
    fn reasoning(&self, _body: &mut Map<String, Value>, _effort: Option<&str>) {}

    /// The constraint kinds the server can enforce on a request.
    fn enforceable(&self) -> &'static [Constraint] {
        &[Constraint::JsonSchema, Constraint::JsonObject]
    }

    /// The output constraint in the server's spelling. The default knows the two JSON shapes.
    fn shape(&self, body: &mut Map<String, Value>, output: &OutputShape) -> Result<(), CodecError> {
        json_shape(body, output)
    }

    /// What only this server understands; the other servers' arms are ignored.
    fn extras(&self, _body: &mut Map<String, Value>, _engine: &EngineExtras) {}
}

fn json_shape(body: &mut Map<String, Value>, output: &OutputShape) -> Result<(), CodecError> {
    match output {
        OutputShape::Free => {}
        OutputShape::JsonSchema(schema) => {
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
        OutputShape::JsonObject => {
            body.insert("response_format".into(), json!({ "type": "json_object" }));
        }
        _ => return Err(CodecError::UnsupportedShape),
    }
    Ok(())
}

/// The chat template's `enable_thinking` switch.
fn enable_thinking(body: &mut Map<String, Value>, effort: Option<&str>) {
    body.insert(
        "chat_template_kwargs".into(),
        json!({ "enable_thinking": effort.is_some() }),
    );
}

struct LlamaServer;
struct Vllm;
struct LiteLlm;
struct OpenRouter;

impl Server for LlamaServer {
    fn honours_named_tool(&self) -> bool {
        false
    }

    fn images_in_tool_message(&self) -> bool {
        true
    }

    /// Where `describe` reads: llama-server keeps `/props` at the server root.
    fn describe_root(&self) -> RouteRoot {
        RouteRoot::Server
    }

    fn honours_dimensions(&self) -> bool {
        false
    }

    fn asks_logprobs(&self) -> bool {
        true
    }

    fn cache_from_timings_first(&self) -> bool {
        true
    }

    fn repeat_penalty_key(&self) -> &'static str {
        "repeat_penalty"
    }

    fn reasoning(&self, body: &mut Map<String, Value>, effort: Option<&str>) {
        enable_thinking(body, effort);
    }

    fn enforceable(&self) -> &'static [Constraint] {
        &[
            Constraint::JsonSchema,
            Constraint::JsonObject,
            Constraint::Gbnf,
            Constraint::Choice,
        ]
    }

    fn shape(&self, body: &mut Map<String, Value>, output: &OutputShape) -> Result<(), CodecError> {
        match output {
            OutputShape::Gbnf(grammar) => {
                body.insert("grammar".into(), json!(grammar));
            }
            OutputShape::Choice(choices) => {
                body.insert("grammar".into(), json!(choice_grammar(choices)));
            }
            other => return json_shape(body, other),
        }
        Ok(())
    }

    fn extras(&self, body: &mut Map<String, Value>, engine: &EngineExtras) {
        if let EngineExtras::LlamaServer(extras) = engine {
            body.insert(
                "cache_prompt".into(),
                json!(extras.cache_prompt == PromptCache::Reuse),
            );
            if let Knob::Set(slot) = extras.slot {
                body.insert("id_slot".into(), json!(slot.0));
            }
        }
    }
}

impl Server for Vllm {
    // To verify against vLLM's structured-output backends.
    fn shape_with_tools(&self) -> ShapeWithTools {
        ShapeWithTools::Together
    }

    fn asks_logprobs(&self) -> bool {
        true
    }

    fn reasoning(&self, body: &mut Map<String, Value>, effort: Option<&str>) {
        enable_thinking(body, effort);
        if let Some(effort) = effort {
            body.insert("reasoning_effort".into(), json!(effort));
        }
    }

    fn enforceable(&self) -> &'static [Constraint] {
        &[
            Constraint::JsonSchema,
            Constraint::JsonObject,
            Constraint::Regex,
            Constraint::Lark,
            Constraint::Choice,
        ]
    }

    fn shape(&self, body: &mut Map<String, Value>, output: &OutputShape) -> Result<(), CodecError> {
        let (key, value) = match output {
            OutputShape::Regex(regex) => ("regex", json!(regex)),
            OutputShape::Lark(grammar) => ("grammar", json!(grammar)),
            OutputShape::Choice(choices) => ("choice", json!(choices)),
            other => return json_shape(body, other),
        };
        let mut inner = Map::new();
        inner.insert(key.into(), value);
        body.insert("structured_outputs".into(), Value::Object(inner));
        Ok(())
    }

    fn extras(&self, body: &mut Map<String, Value>, engine: &EngineExtras) {
        if let EngineExtras::Vllm(extras) = engine
            && let Knob::Set(priority) = extras.priority
        {
            body.insert("priority".into(), json!(priority.0));
        }
    }
}

impl Server for LiteLlm {
    fn reasoning(&self, body: &mut Map<String, Value>, effort: Option<&str>) {
        if let Some(effort) = effort {
            body.insert("reasoning_effort".into(), json!(effort));
        }
    }
}

impl Server for OpenRouter {
    fn asks_usage(&self) -> bool {
        false
    }

    fn charges(&self) -> Charges {
        Charges::PerReply
    }

    fn reasoning(&self, body: &mut Map<String, Value>, effort: Option<&str>) {
        if let Some(effort) = effort {
            body.insert("reasoning".into(), json!({ "effort": effort }));
        }
    }
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
