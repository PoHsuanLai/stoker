//! Builders shared by the session tests. Each test file uses a subset.
#![allow(dead_code)]

use cua_action::{Coord, CuaDialect, GridMax, ModelSpace, Scale120, Size, WindowSpace};
use cua_session::{
    CuaProfile, CuaSession, CuaTaskText, FrameBudget, MaskedRegions, ObservationIn, RepairBudget,
    StepIndex, TurnTranscript,
};
use model_provider::{
    ImageInput, JsonText, ModelName, StopReason, ToolCall, ToolCallId, ToolName, TurnEnd, TurnUsage,
};
use vision_prep::{Encoding, FrameMap, PatchFactor, PixelCount, ResizeRule};

pub const WINDOW_W: u32 = 1280;
pub const WINDOW_H: u32 = 800;

pub fn window() -> Size<WindowSpace> {
    Size::new(Coord(WINDOW_W), Coord(WINDOW_H))
}

pub fn rule() -> ResizeRule {
    ResizeRule::SmartResize {
        factor: PatchFactor(32),
        min_pixels: PixelCount(65_536),
        max_pixels: PixelCount(16_777_216),
    }
}

pub fn map(space: ModelSpace) -> FrameMap {
    FrameMap::new(window(), Scale120(120), &rule(), space).unwrap()
}

pub fn grid() -> ModelSpace {
    ModelSpace::Grid(GridMax(1000))
}

pub fn profile(dialect: CuaDialect, space: ModelSpace, history: u8, repair: u8) -> CuaProfile {
    CuaProfile {
        dialect,
        rule: rule(),
        space,
        history: FrameBudget(history),
        repair: RepairBudget(repair),
        encoding: Encoding::Png,
    }
}

pub fn task() -> CuaTaskText {
    CuaTaskText {
        goal: "save the file".into(),
        hints: vec!["it is in the File menu".into()],
    }
}

pub fn session(dialect: CuaDialect, space: ModelSpace, history: u8, repair: u8) -> CuaSession {
    CuaSession::begin(
        profile(dialect, space, history, repair),
        task(),
        ModelName("holo".into()),
    )
}

/// An image whose bytes are the ASCII of `tag`, so a test can tell frames apart.
pub fn frame(tag: &str) -> ImageInput {
    use base64::Engine as _;
    serde_json::from_value(serde_json::json!({
        "media": "png",
        "bytes": base64::engine::general_purpose::STANDARD.encode(tag.as_bytes()),
        "detail": "auto",
    }))
    .unwrap()
}

pub fn obs(step: u32) -> ObservationIn {
    ObservationIn::new(StepIndex(step), None, vec![], MaskedRegions(0))
}

pub fn end(stop: StopReason) -> TurnEnd {
    TurnEnd {
        stop,
        usage: TurnUsage::default(),
        served: ModelName("holo".into()),
        first_token: None,
    }
}

pub fn call(name: &str, input: &str) -> ToolCall {
    ToolCall {
        id: ToolCallId("c1".into()),
        name: ToolName::new(name).unwrap(),
        input: JsonText::new(input).unwrap(),
    }
}

/// A reply of tool calls to the `computer_use` function.
pub fn calls(inputs: &[&str]) -> TurnTranscript {
    TurnTranscript::new(
        String::new(),
        String::new(),
        inputs.iter().map(|i| call("computer_use", i)).collect(),
        end(StopReason::ToolUse),
    )
}

pub fn said(text: &str) -> TurnTranscript {
    TurnTranscript::new(text.into(), String::new(), vec![], end(StopReason::EndTurn))
}
