//! A model whose calls we read from its text (`CallFormat` other than `Engine`) is told its tools
//! the way its chat template would tell it: in the system message, as JSON function definitions
//! followed by the call format. The definitions are the schema the session already builds for the
//! dialect (`shown`), so no per-model prompt exists; the request then declares no tools, because a
//! server that saw them would run its own parser and strip the text we read the calls from.

use cua_action::{ActionProfile, CallFormat};
use model_provider::ToolSpec;
use serde_json::{Value, json};

use crate::prompt::body;

const QWEN3_XML: &str = include_str!("../prompts/calls_qwen3_coder_xml.txt");
const HERMES_JSON: &str = include_str!("../prompts/calls_hermes_json.txt");

/// What the repair message says about the call format, when we read the calls.
const QWEN3_XML_REMINDER: &str = "Write the call as <tool_call>, then <function=computer_use>, then one <parameter=NAME> block per argument with its value on the next line, each block closed, then </function> and </tool_call>. A call in any other form is not read.";
const HERMES_JSON_REMINDER: &str = "Write the call as <tool_call>{\"name\": \"computer_use\", \"arguments\": {...}}</tool_call>. A call in any other form is not read.";

/// The tools a request declares: none when we read the calls ourselves.
pub(crate) fn declared(actions: ActionProfile, tools: Vec<ToolSpec>) -> Vec<ToolSpec> {
    if actions.calls.is_read_by_us() {
        Vec::new()
    } else {
        tools
    }
}

/// The `# Tools` section of the system message, or `None` when the engine parses the calls.
pub(crate) fn tool_block(format: CallFormat, tools: &[ToolSpec]) -> Option<String> {
    let file = match format {
        CallFormat::Engine => return None,
        CallFormat::Qwen3CoderXml => QWEN3_XML,
        CallFormat::HermesJson => HERMES_JSON,
    };
    Some(format!(
        "# Tools\n\n{}",
        body(file).replace("{tools}", &definitions(tools))
    ))
}

/// The words of the repair message about how a call is written; empty when the engine parses.
pub(crate) fn reminder(format: CallFormat) -> Option<&'static str> {
    match format {
        CallFormat::Engine => None,
        CallFormat::Qwen3CoderXml => Some(QWEN3_XML_REMINDER),
        CallFormat::HermesJson => Some(HERMES_JSON_REMINDER),
    }
}

/// One JSON function definition per line, as a chat template renders `tools`.
fn definitions(tools: &[ToolSpec]) -> String {
    tools
        .iter()
        .filter_map(|spec| match spec {
            ToolSpec::Function {
                name,
                description,
                parameters,
            } => Some(
                json!({
                    "type": "function",
                    "function": {
                        "name": name.as_str(),
                        "description": description,
                        "parameters": serde_json::from_str::<Value>(parameters.0.as_str())
                            .unwrap_or(Value::Null),
                    },
                })
                .to_string(),
            ),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("\n")
}
