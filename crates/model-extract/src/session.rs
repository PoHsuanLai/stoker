//! One extraction, from request to typed value.

use core::marker::PhantomData;

use model_provider::{
    Extract, JsonText, Message, OutputShape, Part, Role, SchemaDialect, Shape, ShapeFault,
    StopReason, ToolCall, ToolChoice, ToolName, ToolSpec, TurnEnd, TurnRequest,
};

use crate::ExtractMode;
use crate::repair::{FINAL_RESULT_DESCRIPTION, prompted_text, repair_text};

/// The name of the synthetic tool of `ExtractMode::ToolCall`.
pub const FINAL_RESULT_TOOL: &str = "final_result";

/// How many repairs a session may spend. A setting the daemon reads; no default lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepairBudget(pub u8);

/// Repairs still available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepairsLeft(pub u8);

/// Why an extraction ended without a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtractFailure {
    /// The reply never fit the shape within the budget (`ModelError::Unparseable`).
    Unparseable,
    /// `StopReason::MaxTokens`: never repaired, so a long answer is not retried into the same cut.
    Truncated,
    /// The model refused or the content filter stopped it.
    Refused,
    /// No repair is left.
    OverBudget,
}

/// What `absorb` made of a reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extracted<T> {
    Done(T),
    /// Send this request instead: the base turn plus the fault, as a tool result or a user
    /// message.
    Repair(Box<TurnRequest>),
    Failed(ExtractFailure),
}

/// One extraction of a `T`: the mode, the repairs left. Pure: `request` and `absorb` are total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractSession<T: Extract> {
    mode: ExtractMode,
    left: RepairsLeft,
    last_fault: Option<ShapeFault>,
    of: PhantomData<fn() -> T>,
}

impl<T: Extract> ExtractSession<T> {
    pub fn new(mode: ExtractMode, budget: RepairBudget) -> Self {
        Self {
            mode,
            left: RepairsLeft(budget.0),
            last_fault: None,
            of: PhantomData,
        }
    }

    pub fn mode(&self) -> &ExtractMode {
        &self.mode
    }

    pub fn left(&self) -> RepairsLeft {
        self.left
    }

    /// `base` with this session's output shape, tools and tool choice set; after a fault, with
    /// the fixed repair turn appended (it never repeats what the model said). A request that
    /// carries tools of its own keeps them, and `limits` are the base's.
    pub fn request(&self, base: &TurnRequest) -> TurnRequest {
        let shape = T::shape();
        let mut request = base.clone();
        match &self.mode {
            ExtractMode::Native(output) => request.output = output.clone(),
            ExtractMode::ToolCall { tool } => {
                request.output = OutputShape::Free;
                request.tool_choice = ToolChoice::Required;
                request.tools.retain(|t| !is_named(t, tool));
                request.tools.push(ToolSpec::Function {
                    name: tool.clone(),
                    description: FINAL_RESULT_DESCRIPTION.to_owned(),
                    parameters: shape.to_json_schema(SchemaDialect::Plain),
                });
            }
            ExtractMode::Prompted => {
                request.output = OutputShape::Free;
                let schema = shape.to_json_schema(SchemaDialect::Plain);
                let text = Part::Text(prompted_text(schema.0.as_str()));
                match request.messages.first_mut() {
                    Some(first) if first.role == Role::System => first.parts.push(text),
                    _ => request.messages.insert(
                        0,
                        Message {
                            role: Role::System,
                            parts: vec![text],
                        },
                    ),
                }
            }
        }
        if let Some(fault) = &self.last_fault {
            request.messages.push(Message {
                role: Role::User,
                parts: vec![Part::Text(repair_text(fault))],
            });
        }
        request
    }

    /// Reads a finished turn: its end, its text, its tool calls. `base` is the request that was
    /// sent first; a fault with a repair left answers `Repair` of `request(base)` with the fault
    /// appended.
    ///
    /// Truncation is checked before anything else and is never repaired. A reply that does not
    /// fit is a repair while the budget lasts; with the budget spent it is `Unparseable` (a
    /// repair was tried) or `OverBudget` (none was ever available).
    pub fn absorb(
        &mut self,
        base: &TurnRequest,
        end: &TurnEnd,
        text: &str,
        calls: &[ToolCall],
    ) -> Extracted<T> {
        match end.stop {
            StopReason::MaxTokens => return Extracted::Failed(ExtractFailure::Truncated),
            StopReason::ContentFilter => return Extracted::Failed(ExtractFailure::Refused),
            StopReason::EndTurn | StopReason::ToolUse | StopReason::StopSequence => {}
        }
        let fault = match self
            .candidate(text, calls)
            .and_then(|json| read::<T>(&json))
        {
            Ok(value) => return Extracted::Done(value),
            Err(fault) => fault,
        };
        match self.left.0.checked_sub(1) {
            Some(left) => {
                self.left = RepairsLeft(left);
                self.last_fault = Some(fault);
                Extracted::Repair(Box::new(self.request(base)))
            }
            None if self.last_fault.is_some() => Extracted::Failed(ExtractFailure::Unparseable),
            None => Extracted::Failed(ExtractFailure::OverBudget),
        }
    }

    /// The JSON the reply holds, by mode.
    fn candidate(&self, text: &str, calls: &[ToolCall]) -> Result<JsonText, ShapeFault> {
        match &self.mode {
            ExtractMode::ToolCall { tool } => calls
                .iter()
                .find(|c| &c.name == tool)
                .map(|c| c.input.clone())
                .ok_or(ShapeFault::NotJson),
            ExtractMode::Native(OutputShape::Choice(_) | OutputShape::Regex(_)) => {
                bare_value(&T::shape(), text)
            }
            ExtractMode::Native(_) | ExtractMode::Prompted => {
                JsonText::new(unfenced(text)).map_err(|_| ShapeFault::NotJson)
            }
        }
    }
}

fn is_named(spec: &ToolSpec, tool: &ToolName) -> bool {
    matches!(spec, ToolSpec::Function { name, .. } if name == tool)
}

/// The shape's own check, then the type's reader.
fn read<T: Extract>(json: &JsonText) -> Result<T, ShapeFault> {
    T::shape().check(json)?;
    T::read(json)
}

/// A reply constrained to a bare value (`allow`, `7`) as the JSON of its shape: a choice is a
/// string, an integer is itself.
fn bare_value(shape: &Shape, text: &str) -> Result<JsonText, ShapeFault> {
    let text = text.trim();
    let json = match shape {
        Shape::Choice(_) => serde_json::Value::String(text.to_owned()).to_string(),
        _ => text.to_owned(),
    };
    JsonText::new(json).map_err(|_| ShapeFault::NotJson)
}

/// `text` without one surrounding Markdown code fence, which models add to a JSON reply.
fn unfenced(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix("```")
        .and_then(|rest| rest.strip_suffix("```"))
        .map(|body| body.strip_prefix("json").unwrap_or(body).trim())
        .unwrap_or(text)
}
