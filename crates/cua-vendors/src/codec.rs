//! The codec trait and the closed set of codecs.

use cua_action::{CoordSpace, CuaAction, ImageSpace, Size, WireDialect};
use cua_parse::{Batch, DropReason, InSpace, ParseLimits, Parsed};
use model_provider::{ImageInput, JsonText, NativeTool, Part, SafetySignal, ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::anthropic::AnthropicTool;
use crate::args::{self, Verdict};
use crate::safety::apply_safety;
use crate::{StepResult, anthropic, gemini, openai, results};

/// One vendor's computer-use encoding.
pub trait WireCodec {
    /// The tool declaration: a `NativeTool` or `Function` per dialect, for an image of this size.
    fn tools(&self, image: Size<ImageSpace>) -> Vec<ToolSpec>;
    /// Decodes the model's calls. Vendor safety hints may only add asks.
    fn decode(&self, calls: &[ToolCall], safety: &[SafetySignal]) -> Result<Parsed, WireError>;
    /// Encodes the results of the previous step and the next observation as the vendor wants
    /// them (`tool_result`, `computer_call_output`, `function_response`).
    fn results(&self, done: &[StepResult], next: &ImageInput) -> Vec<Part>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum WireError {
    #[error("the call names a tool this dialect does not have")]
    UnknownTool,
    #[error("the call's arguments do not fit the dialect's schema")]
    BadArguments,
}

/// The codecs, one per [`WireDialect`]; a closed set, so an enum rather than `dyn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireCodecs {
    AnthropicToolset20260801,
    AnthropicComputer20251124,
    OpenAiComputer,
    GeminiComputerUse,
}

impl WireCodecs {
    pub fn dialect(&self) -> WireDialect {
        match self {
            WireCodecs::AnthropicToolset20260801 => WireDialect::AnthropicToolset20260801,
            WireCodecs::AnthropicComputer20251124 => WireDialect::AnthropicComputer20251124,
            WireCodecs::OpenAiComputer => WireDialect::OpenAiComputer,
            WireCodecs::GeminiComputerUse => WireDialect::GeminiComputerUse,
        }
    }
}

/// The codec for a dialect.
pub fn codec(dialect: WireDialect) -> WireCodecs {
    match dialect {
        WireDialect::AnthropicToolset20260801 => WireCodecs::AnthropicToolset20260801,
        WireDialect::AnthropicComputer20251124 => WireCodecs::AnthropicComputer20251124,
        WireDialect::OpenAiComputer => WireCodecs::OpenAiComputer,
        WireDialect::GeminiComputerUse => WireCodecs::GeminiComputerUse,
    }
}

impl WireCodec for WireCodecs {
    fn tools(&self, image: Size<ImageSpace>) -> Vec<ToolSpec> {
        let config = match self {
            WireCodecs::AnthropicToolset20260801 => json!({"type": "computer_toolset_20260801"}),
            WireCodecs::AnthropicComputer20251124 => json!({
                "type": "computer_20251124",
                "name": "computer",
                "display_width_px": image.w.0,
                "display_height_px": image.h.0,
                "enable_zoom": true,
            }),
            WireCodecs::OpenAiComputer => json!({"type": "computer"}),
            WireCodecs::GeminiComputerUse => {
                json!({"type": "computer_use", "environment": "desktop"})
            }
        };
        let config = JsonText::from_value(&config);
        vec![ToolSpec::Native(NativeTool {
            dialect: self.dialect(),
            config,
        })]
    }

    fn decode(&self, calls: &[ToolCall], safety: &[SafetySignal]) -> Result<Parsed, WireError> {
        let mut parsed = match self {
            WireCodecs::AnthropicToolset20260801 => {
                decode_anthropic(calls, AnthropicTool::Toolset)?
            }
            WireCodecs::AnthropicComputer20251124 => {
                decode_anthropic(calls, AnthropicTool::Computer2025)?
            }
            WireCodecs::OpenAiComputer => decode_openai(calls)?,
            WireCodecs::GeminiComputerUse => decode_gemini(calls)?,
        };
        parsed.actions = apply_safety(parsed.actions, safety);
        Ok(parsed)
    }

    fn results(&self, done: &[StepResult], next: &ImageInput) -> Vec<Part> {
        match self {
            WireCodecs::AnthropicToolset20260801 | WireCodecs::AnthropicComputer20251124 => {
                results::anthropic(done, next)
            }
            WireCodecs::OpenAiComputer | WireCodecs::GeminiComputerUse => {
                if done.is_empty() {
                    vec![Part::Image(next.clone())]
                } else {
                    results::with_screenshot_each(done, next)
                }
            }
        }
    }
}

/// The calls of one reply read into actions. `per_call` gives each call's verb and what it
/// became, or `None` for a call that is no tool of this dialect at all.
struct Tally<S: CoordSpace> {
    batch: Batch<S>,
    calls: usize,
    unknown: usize,
    unreadable: usize,
}

impl<S: CoordSpace> Tally<S> {
    fn new() -> Self {
        Tally {
            batch: Batch::new(ParseLimits::default()),
            calls: 0,
            unknown: 0,
            unreadable: 0,
        }
    }

    fn unknown(&mut self, verb: &str) {
        self.calls += 1;
        self.unknown += 1;
        self.batch.push(verb, Err(DropReason::UnsupportedVerb));
    }

    fn unreadable(&mut self, verb: &str) {
        self.calls += 1;
        self.unreadable += 1;
        self.batch.push(verb, Err(DropReason::BadArgument));
    }

    fn push(&mut self, verb: &str, action: Verdict<CuaAction<S>>) {
        self.batch.push(verb, action);
    }

    /// Every call being no tool of the dialect, or no JSON object, is a fault of the whole
    /// reply; anything else is per-call drops in the `Parsed`.
    fn finish(self, wrap: impl FnOnce(Vec<CuaAction<S>>) -> InSpace) -> Result<Parsed, WireError> {
        if self.calls > 0 && self.unreadable == self.calls {
            return Err(WireError::BadArguments);
        }
        if self.calls > 0 && self.unknown + self.unreadable == self.calls {
            return Err(WireError::UnknownTool);
        }
        self.batch
            .into_parsed(None, wrap)
            .map_err(|_| WireError::UnknownTool)
    }
}

fn nothing() -> Parsed {
    Parsed {
        thought: None,
        actions: InSpace::Image(Vec::new()),
        dropped: Vec::new(),
    }
}

fn decode_anthropic(calls: &[ToolCall], tool: AnthropicTool) -> Result<Parsed, WireError> {
    if calls.is_empty() {
        return Ok(nothing());
    }
    let mut tally = Tally::<ImageSpace>::new();
    for call in calls {
        let name = call.name.as_str();
        let Some(args) = args::object(call.input.as_str()) else {
            tally.unreadable(name);
            continue;
        };
        match anthropic::member(name, &args, tool) {
            Some(verb) if anthropic::is_member(&verb) => {
                tally.push(&verb, anthropic::action(&verb, &args));
            }
            Some(verb) => tally.unknown(&verb),
            None => tally.unknown(name),
        }
    }
    tally.finish(InSpace::Image)
}

fn decode_openai(calls: &[ToolCall]) -> Result<Parsed, WireError> {
    if calls.is_empty() {
        return Ok(nothing());
    }
    let mut tally = Tally::<ImageSpace>::new();
    for call in calls {
        let name = call.name.as_str();
        let Some(args) = args::object(call.input.as_str()) else {
            tally.unreadable(name);
            continue;
        };
        if !matches!(name, "computer" | "computer_call") {
            tally.unknown(name);
            continue;
        }
        let actions = openai::actions_of(&args);
        if actions.is_empty() {
            tally.unreadable(name);
        }
        for (verb, action) in actions {
            tally.push(&verb, action);
        }
    }
    tally.finish(InSpace::Image)
}

fn decode_gemini(calls: &[ToolCall]) -> Result<Parsed, WireError> {
    if calls.is_empty() {
        return Ok(nothing());
    }
    let mut tally = Tally::new();
    for call in calls {
        let name = call.name.as_str();
        let Some(args) = args::object(call.input.as_str()) else {
            tally.unreadable(name);
            continue;
        };
        if gemini::is_function(name) {
            tally.push(name, gemini::action(name, &args));
        } else {
            tally.unknown(name);
        }
    }
    tally.finish(|actions| InSpace::Grid(gemini::GRID_MAX, actions))
}
