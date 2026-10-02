//! One extraction, from request to typed value.

use core::marker::PhantomData;

use model_provider::{Extract, ShapeFault, ToolCall, TurnEnd, TurnRequest};

use crate::ExtractMode;

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

    /// `base` with this session's output shape, tools, tool choice and limits set.
    pub fn request(&self, base: &TurnRequest) -> TurnRequest {
        let _ = (&self.mode, &self.last_fault, base);
        todo!("ExtractSession::request: output, synthetic tool, tool_choice, limits")
    }

    /// Reads a finished turn: its end, its text, its tool calls.
    pub fn absorb(&mut self, end: &TurnEnd, text: &str, calls: &[ToolCall]) -> Extracted<T> {
        let _ = (&mut self.left, &mut self.last_fault, end, text, calls);
        todo!("ExtractSession::absorb: Truncated before anything, then T::read, then one repair")
    }
}
