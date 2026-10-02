//! The state and the values that go in and out of a session (data only).

use cua_action::{CuaAction, CuaDialect, ModelSpace, Point, WindowSpace};
use cua_parse::{Dropped, ParseError};
use cua_vendors::StepResult;
use model_provider::{ToolCall, TurnEnd, TurnRequest};
use vision_prep::{Encoding, ResizeRule};

/// The index of a step within one run, from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepIndex(pub u32);

/// How many regions of the window were masked out of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaskedRegions(pub u16);

/// How many past frames the prompt keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameBudget(pub u8);

/// How many repair prompts a step may spend on a reply that does not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepairBudget(pub u8);

/// How one model is prompted and parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuaProfile {
    pub dialect: CuaDialect,
    pub rule: ResizeRule,
    pub space: ModelSpace,
    pub history: FrameBudget,
    pub repair: RepairBudget,
    pub encoding: Encoding,
}

/// The task as the planner states it: typed text from the planner, not a raw untrusted blob.
#[derive(Clone, PartialEq, Eq)]
pub struct CuaTaskText {
    pub goal: String,
    pub hints: Vec<String>,
}

// The goal is what the person asked for: Debug shows sizes only.
impl core::fmt::Debug for CuaTaskText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "CuaTaskText(<goal {} chars, {} hints>)",
            self.goal.chars().count(),
            self.hints.len()
        )
    }
}

/// What the runner saw before this step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationIn {
    pub step: StepIndex,
    pub cursor: Option<Point<WindowSpace>>,
    pub prev: Vec<StepResult>,
    pub masked: MaskedRegions,
}

/// The model's whole reply to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTranscript {
    pub text: String,
    pub thought: String,
    pub calls: Vec<ToolCall>,
    pub end: TurnEnd,
}

/// What a reply became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Actions {
        thought: Option<String>,
        actions: Vec<CuaAction<WindowSpace>>,
        dropped: Vec<Dropped>,
    },
    /// The reply did not parse and a repair is left: send this prompt (it carries no new frame).
    Repair(TurnRequest),
    /// The reply did not parse and no repair is left. It counts as a step; cuad decides.
    Unparseable(ParseError),
}
