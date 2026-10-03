//! The state and the values that go in and out of a session (data only).

use cua_action::{CuaAction, CuaDialect, ModelSpace, Point, WindowSpace};
use cua_parse::{Dropped, ParseError};
use cua_vendors::StepResult;
use model_provider::{
    EngineExtras, ImageCount, ImageLimits, Knob, Limits, Milli, Reasoning, SafetySignal, Sampling,
    Tokens, ToolCall, ToolParallelism, TurnEnd, TurnRequest,
};
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

impl FrameBudget {
    /// The most past frames a prompt of `per_prompt` images can keep: the current frame is one of
    /// the images, so a model limited to three (Holo's `--limit-mm-per-prompt image:3`) keeps
    /// two earlier ones. A model that takes no image keeps none.
    pub fn within(per_prompt: ImageCount) -> FrameBudget {
        FrameBudget(u8::try_from(per_prompt.0.saturating_sub(1)).unwrap_or(u8::MAX))
    }

    /// `wanted` (the setting `ai.cua.history_frames`), cut to what `per_prompt` allows.
    pub fn at_most(self, per_prompt: ImageCount) -> FrameBudget {
        FrameBudget(self.0.min(FrameBudget::within(per_prompt).0))
    }
}

/// How many repair prompts a step may spend on a reply that does not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepairBudget(pub u8);

/// How many earlier steps the prompt lists as one line each (the frames are `FrameBudget`'s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StepLines(pub u8);

/// What a step's request says beyond the prompt: how the model samples and how long it may
/// answer. `CuaProfile` is frozen and carries none of it, so the daemon sets it from the model's
/// catalog entry with [`CuaSession::with_settings`](crate::CuaSession::with_settings); a session
/// that is never given settings uses [`TurnSettings::default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSettings {
    pub sampling: Sampling,
    pub limits: Limits,
    pub reasoning: Reasoning,
    pub tool_calls: ToolParallelism,
    pub engine: EngineExtras,
    pub lines: StepLines,
}

impl Default for TurnSettings {
    /// Greedy sampling (the same screen gives the same click), no reasoning, one call per turn,
    /// 1024 tokens of answer, eight earlier steps listed. Provisional: the daemon replaces them
    /// with the catalog's `reasoning_off` sampling and `max_output`.
    fn default() -> Self {
        TurnSettings {
            sampling: Sampling {
                temperature: Milli(0),
                top_p: Knob::Off,
                top_k: Knob::Off,
                min_p: Knob::Off,
                repeat_penalty: Knob::Off,
                seed: Knob::Off,
            },
            limits: Limits {
                max_output: Tokens(1024),
                stop: Vec::new(),
            },
            reasoning: Reasoning::Off,
            tool_calls: ToolParallelism::One,
            engine: EngineExtras::None,
            lines: StepLines(8),
        }
    }
}

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

impl CuaProfile {
    /// The profile of a model whose catalog entry says how it takes images: the resize rule and
    /// the point space come from `images`, and the history is `wanted` cut to
    /// `images.per_prompt - 1` frames (the current frame is one of the prompt's images).
    pub fn for_model(
        dialect: CuaDialect,
        images: &ImageLimits,
        wanted: FrameBudget,
        repair: RepairBudget,
        encoding: Encoding,
    ) -> CuaProfile {
        CuaProfile {
            dialect,
            rule: images.rule,
            space: images.space,
            history: wanted.at_most(images.per_prompt),
            repair,
            encoding,
        }
    }
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

/// What the window's own accessibility tree says, as text (window contents, never a secret field
/// the runner already masked). It is what the person sees, so `Debug` prints a length.
#[derive(Clone, PartialEq, Eq)]
pub struct TreeText(pub String);

impl core::fmt::Debug for TreeText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TreeText(<{} chars>)", self.0.chars().count())
    }
}

/// One line the runner adds about this step (a dialog appeared, the focus moved). Short text of
/// the runner's own words; `Debug` prints a length because a line can name a window.
#[derive(Clone, PartialEq, Eq)]
pub struct StepNote(pub String);

impl core::fmt::Debug for StepNote {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "StepNote(<{} chars>)", self.0.chars().count())
    }
}

/// What the runner saw before this step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationIn {
    pub step: StepIndex,
    pub cursor: Option<Point<WindowSpace>>,
    pub prev: Vec<StepResult>,
    pub masked: MaskedRegions,
    /// The window's contents as text, when the toolkit gives one.
    pub tree: Option<TreeText>,
    /// What the runner noticed since the last step, one line each.
    pub notes: Vec<StepNote>,
}

impl ObservationIn {
    /// An observation with no tree and no notes: the form every step had before they existed.
    pub fn new(
        step: StepIndex,
        cursor: Option<Point<WindowSpace>>,
        prev: Vec<StepResult>,
        masked: MaskedRegions,
    ) -> ObservationIn {
        ObservationIn {
            step,
            cursor,
            prev,
            masked,
            tree: None,
            notes: Vec::new(),
        }
    }

    pub fn with_tree(self, tree: TreeText) -> ObservationIn {
        ObservationIn {
            tree: Some(tree),
            ..self
        }
    }

    pub fn with_notes(self, notes: Vec<StepNote>) -> ObservationIn {
        ObservationIn { notes, ..self }
    }
}

/// The model's whole reply to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTranscript {
    pub text: String,
    pub thought: String,
    pub calls: Vec<ToolCall>,
    pub end: TurnEnd,
    /// What the vendor's safety layer said about the turn. A vendor wire reads it (a block ends
    /// the run, a confirmation request becomes an ask); the other dialects ignore it.
    pub safety: Vec<SafetySignal>,
}

impl TurnTranscript {
    /// A reply with no safety signals.
    pub fn new(text: String, thought: String, calls: Vec<ToolCall>, end: TurnEnd) -> Self {
        TurnTranscript {
            text,
            thought,
            calls,
            end,
            safety: Vec::new(),
        }
    }
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
