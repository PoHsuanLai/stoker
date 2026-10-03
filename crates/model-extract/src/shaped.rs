//! One extraction of a shape known only at run time: a schema a caller holds as text.

use model_provider::{
    JsonText, SchemaLimits, SchemaRefusal, SchemaText, Shape, ToolCall, TurnEnd, TurnRequest,
};

use crate::session::Machine;
use crate::{ExtractMode, Extracted, RepairBudget, RepairsLeft};

/// [`ExtractSession`](crate::ExtractSession) for a `Shape` held as a value: the reply that comes
/// back has passed `Shape::check` and is handed on as the JSON it was. The mode, the one-repair
/// rule and the failure reasons are the typed session's, and so is the machine under both.
///
/// Built from schema text with [`ShapedSession::from_schema`], which refuses a schema the shape
/// vocabulary cannot say: such a request is sent with its schema to the engine and not validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedSession {
    shape: Shape,
    machine: Machine,
}

impl ShapedSession {
    pub fn new(shape: Shape, mode: ExtractMode, budget: RepairBudget) -> Self {
        Self {
            shape,
            machine: Machine::new(mode, budget),
        }
    }

    /// A session for the shape of `schema`, or why the schema is not one.
    pub fn from_schema(
        schema: &SchemaText,
        limits: SchemaLimits,
        mode: ExtractMode,
        budget: RepairBudget,
    ) -> Result<Self, SchemaRefusal> {
        Shape::from_json_schema(schema, limits).map(|shape| Self::new(shape, mode, budget))
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn mode(&self) -> &ExtractMode {
        &self.machine.mode
    }

    pub fn left(&self) -> RepairsLeft {
        self.machine.left
    }

    /// As [`ExtractSession::request`](crate::ExtractSession::request).
    pub fn request(&self, base: &TurnRequest) -> TurnRequest {
        self.machine.request(base, &self.shape)
    }

    /// As [`ExtractSession::absorb`](crate::ExtractSession::absorb); `Done` holds the checked
    /// JSON.
    pub fn absorb(
        &mut self,
        base: &TurnRequest,
        end: &TurnEnd,
        text: &str,
        calls: &[ToolCall],
    ) -> Extracted<JsonText> {
        self.machine
            .absorb(base, end, text, calls, &self.shape, |json| Ok(json.clone()))
    }
}
