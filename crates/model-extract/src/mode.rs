//! How to ask.

use model_provider::{
    Caps, Constraint, OutputShape, SchemaDialect, Shape, ShapeWithTools, ToolName, ToolSupport,
};

use crate::FINAL_RESULT_TOOL;

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
/// 1. `caps.output` has `Choice`, `Gbnf` or `Regex` for a `Choice` or bounded `Integer` shape:
///    `Native` of it (`Choice`, then `Regex`, then `Gbnf`).
/// 2. `caps.output` has `JsonSchema`, or `Gbnf` for a shape GBNF can say: `Native` of it.
/// 3. `caps.tools` is not `Absent`: `ToolCall` (the synthetic `final_result`, never a named
///    `tool_choice`, which llama-server ignores).
/// 4. `Prompted`.
///
/// Rules 1 and 2 hold only when the request has no tools of its own or the engine composes a
/// constrained reply with tools (`with_tools` is `Together`): a constraint over the whole reply
/// would suppress the tool calls. `with_tools` is the engine flavor's property
/// (`Quirks::shape_with_tools` in the codec crate).
pub fn choose(
    caps: &Caps,
    shape: &Shape,
    tools: ToolsPresent,
    with_tools: ShapeWithTools,
) -> ExtractMode {
    let composes = tools == ToolsPresent::No || with_tools == ShapeWithTools::Together;
    let native = composes.then(|| scalar(caps, shape).or_else(|| general(caps, shape)));
    native.flatten().unwrap_or_else(|| match caps.tools {
        ToolSupport::Absent => ExtractMode::Prompted,
        ToolSupport::Native | ToolSupport::ServerParsed => ExtractMode::ToolCall {
            tool: ToolName::new(FINAL_RESULT_TOOL)
                .unwrap_or_else(|_| unreachable!("the synthetic tool name is valid")),
        },
    })
}

/// Rule 1: a choice or a bounded integer, by the narrowest constraint the engine has.
fn scalar(caps: &Caps, shape: &Shape) -> Option<ExtractMode> {
    let has = |c: Constraint| caps.output.contains(&c);
    let choice = match shape {
        Shape::Choice(items) if has(Constraint::Choice) && !items.is_empty() => Some(
            OutputShape::Choice(items.iter().map(|c| c.0.clone()).collect()),
        ),
        _ => None,
    };
    let regex = || {
        has(Constraint::Regex)
            .then(|| shape.to_regex().map(OutputShape::Regex))
            .flatten()
    };
    let gbnf = || gbnf_of(caps, shape);
    let is_scalar = matches!(shape, Shape::Choice(_) | Shape::Integer { .. });
    is_scalar
        .then(|| choice.or_else(regex).or_else(gbnf))
        .flatten()
        .map(ExtractMode::Native)
}

/// Rule 2: the schema where the engine takes one, else a grammar.
fn general(caps: &Caps, shape: &Shape) -> Option<ExtractMode> {
    caps.output
        .contains(&Constraint::JsonSchema)
        .then(|| OutputShape::JsonSchema(shape.to_json_schema(SchemaDialect::Plain)))
        .or_else(|| gbnf_of(caps, shape))
        .map(ExtractMode::Native)
}

fn gbnf_of(caps: &Caps, shape: &Shape) -> Option<OutputShape> {
    caps.output
        .contains(&Constraint::Gbnf)
        .then(|| shape.to_gbnf().ok().map(OutputShape::Gbnf))
        .flatten()
}
