//! Shared builders. Each test file uses a subset.
#![allow(dead_code)]

use cua_action::{GridMax, ModelSpace};
use model_provider::{
    Caps, Constraint, CuaSupport, EngineExtras, ImageCount, ImageLimits, InputKind, Knob, Limits,
    Message, Milli, ModelName, OutputShape, Part, Reasoning, Role, Sampling, StopReason, Support,
    Tokens, ToolChoice, ToolParallelism, ToolSupport, TurnEnd, TurnRequest, TurnUsage,
};
use vision_prep::{PatchFactor, PixelCount, ResizeRule};

pub fn caps(output: &[Constraint], tools: ToolSupport) -> Caps {
    Caps {
        inputs: [InputKind::Text].into(),
        tools,
        output: output.iter().copied().collect(),
        reasoning: Support::Absent,
        streaming: Support::Present,
        images: ImageLimits {
            per_prompt: ImageCount(0),
            rule: ResizeRule::SmartResize {
                factor: PatchFactor(32),
                min_pixels: PixelCount(1),
                max_pixels: PixelCount(2),
            },
            space: ModelSpace::Grid(GridMax(1000)),
        },
        context: Tokens(8192),
        max_output: Tokens(512),
        computer_use: CuaSupport::Absent,
    }
}

pub fn base() -> TurnRequest {
    TurnRequest {
        model: ModelName("m".into()),
        messages: vec![Message {
            role: Role::User,
            parts: vec![Part::Text("judge".into())],
        }],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        tool_calls: ToolParallelism::One,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(64),
            stop: vec![],
        },
        sampling: Sampling {
            temperature: Milli(0),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::Off,
        engine: EngineExtras::None,
        choice_scores: model_provider::ChoiceScores::Off,
    }
}

pub fn end(stop: StopReason) -> TurnEnd {
    TurnEnd {
        stop,
        usage: TurnUsage::default(),
        served: ModelName("m".into()),
        first_token: None,
    }
}
