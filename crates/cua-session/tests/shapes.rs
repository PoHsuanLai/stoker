use cua_action::{CuaDialect, GridMax, ModelSpace, ToolDialect};
use cua_session::{CuaProfile, CuaSession, CuaTaskText, FrameBudget, RepairBudget};
use model_provider::ModelName;
use vision_prep::{Encoding, PatchFactor, PixelCount, ResizeRule};

fn profile() -> CuaProfile {
    CuaProfile {
        dialect: CuaDialect::Tool(ToolDialect::Holo31),
        rule: ResizeRule::SmartResize {
            factor: PatchFactor(32),
            min_pixels: PixelCount(65_536),
            max_pixels: PixelCount(16_777_216),
        },
        space: ModelSpace::Grid(GridMax(1000)),
        history: FrameBudget(3),
        repair: RepairBudget(1),
        encoding: Encoding::Png,
    }
}

#[test]
fn begin_builds_a_session_without_touching_the_model() {
    let task = CuaTaskText {
        goal: "save the file".into(),
        hints: vec![],
    };
    let session = CuaSession::begin(profile(), task.clone(), ModelName("holo".into()));
    assert_eq!(session.clone(), session);
    assert_eq!(format!("{task:?}"), "CuaTaskText(<goal 13 chars, 0 hints>)");
}
