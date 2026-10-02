//! How to ask.

use model_provider::{Caps, OutputShape, Shape, ShapeWithTools, ToolName};

/// How a request asks for a shaped reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractMode {
    /// The engine enforces it: the request carries this `OutputShape`.
    Native(OutputShape),
    /// A synthetic tool whose parameters are the schema, with `ToolChoice::Required`.
    ToolCall { tool: ToolName },
    /// The schema in the system prompt; the reply is validated strictly.
    Prompted,
}

/// Whether the request carries function tools of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolsPresent {
    No,
    Yes,
}

/// Picks the mode, in this order:
///
/// 1. `caps.output` has `Choice`, `Gbnf` or `Regex` for a `Choice` or bounded shape: `Native` of it.
/// 2. `caps.output` has `JsonSchema` and there are no tools, or `with_tools` is `Together`:
///    `Native(JsonSchema)`.
/// 3. `caps.tools` is not `Absent`: `ToolCall` (the synthetic `final_result`, never a named
///    `tool_choice`, which llama-server ignores).
/// 4. `Prompted`.
///
/// `with_tools` is the engine flavor's property (`Quirks::shape_with_tools` in the codec crate).
pub fn choose(
    caps: &Caps,
    shape: &Shape,
    tools: ToolsPresent,
    with_tools: ShapeWithTools,
) -> ExtractMode {
    let _ = (caps, shape, tools, with_tools);
    todo!("choose: the four rules, in order")
}
