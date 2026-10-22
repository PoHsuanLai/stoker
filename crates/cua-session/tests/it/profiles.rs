//! A model's action profile in a session: the schema it is shown names the verbs it was trained
//! on, and a drag from the cursor needs the step's cursor.

use crate::support;

use cua_action::{
    ActionProfile, Coord, CuaAction, CuaDialect, DragStyle, Point, ScrollAxis, ToolDialect,
    VerbAlias,
};
use cua_parse::DropReason;
use cua_session::{CuaSession, StepOutcome};
use model_provider::{ModelName, Part, ToolSpec};
use support::*;

const QWEN: CuaDialect = CuaDialect::Tool(ToolDialect::QwenComputerUse);

fn ui_mate() -> ActionProfile {
    ActionProfile {
        aliases: vec![
            VerbAlias::Drag,
            VerbAlias::Finished,
            VerbAlias::Hotkey,
            VerbAlias::Press,
            VerbAlias::CallUser,
        ]
        .into(),
        drag: DragStyle::FromCursor,
        scroll: ScrollAxis::ByDirection,
    }
}

fn session_with(actions: ActionProfile) -> CuaSession {
    let profile = profile(QWEN, grid(), 0, 0).with_actions(actions);
    CuaSession::begin(profile, task(), ModelName("ui-mate".into()))
}

/// The system text, and the declared function's description and parameters.
fn shown(actions: ActionProfile) -> (String, String, serde_json::Value) {
    let request = session_with(actions).request(&obs(0), &map(grid()), frame("f"));
    let Part::Text(system) = &request.messages[0].parts[0] else {
        panic!("a system text")
    };
    let [
        ToolSpec::Function {
            description,
            parameters,
            ..
        },
    ] = &request.tools[..]
    else {
        panic!("one function")
    };
    let schema = serde_json::from_str(parameters.0.as_str()).unwrap();
    (system.clone(), description.clone(), schema)
}

#[test]
fn the_schema_names_the_verbs_of_the_profile() {
    let (system, _, schema) = shown(ui_mate());
    let action = &schema["properties"]["action"];
    let verbs: Vec<&str> = action["enum"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        verbs,
        [
            "left_click",
            "right_click",
            "middle_click",
            "double_click",
            "triple_click",
            "mouse_move",
            "drag",
            "type",
            "hotkey",
            "press",
            "scroll",
            "wait",
            "finished",
            "answer",
            "call_user",
        ]
    );
    let lines = action["description"].as_str().unwrap();
    for verb in &verbs {
        assert!(
            lines.contains(&format!("* `{verb}`: ")),
            "{verb} is described"
        );
    }
    for gone in ["left_click_drag", "terminate", "hscroll", "`key`"] {
        assert!(!lines.contains(gone), "{gone} is not shown");
    }
    assert!(lines.contains("where the cursor is"), "{lines}");
    let properties = schema["properties"].as_object().unwrap();
    assert!(!properties.contains_key("start_coordinate"));
    assert!(properties.contains_key("direction"));
    assert!(
        properties["status"]["description"]
            .as_str()
            .unwrap()
            .contains("`finished`")
    );
    assert!(system.contains("action finished and a status"), "{system}");
}

#[test]
fn a_model_without_a_profile_is_shown_the_common_schema_unchanged() {
    let (system, _, schema) = shown(ActionProfile::default());
    let file = include_str!("../../prompts/computer_use.schema.json");
    assert_eq!(
        schema,
        serde_json::from_str::<serde_json::Value>(file).unwrap()
    );
    assert!(system.contains("action terminate and a status"), "{system}");
}

#[test]
fn a_drag_from_the_cursor_uses_the_cursor_of_the_step() {
    let reply = || calls(&[r#"{"action":"drag","coordinate":[600,700]}"#]);
    let map = map(grid());
    let mut s = session_with(ui_mate());
    let o = obs(0);
    let sent = s.request(&o, &map, frame("f"));
    let at = Some(Point::new(Coord(640), Coord(400)));
    let StepOutcome::Actions {
        actions, dropped, ..
    } = s.absorb_at(&sent, reply(), &map, at)
    else {
        panic!("actions")
    };
    assert!(dropped.is_empty(), "{dropped:?}");
    assert!(
        matches!(&actions[..], [CuaAction::Drag { .. }]),
        "{actions:?}"
    );
    // Without a cursor the drag has no start and is dropped, whatever the call says.
    let StepOutcome::Actions {
        actions, dropped, ..
    } = s.absorb_for(&sent, reply(), &map)
    else {
        panic!("actions")
    };
    assert!(actions.is_empty());
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].reason, DropReason::MissingArgument);
}
