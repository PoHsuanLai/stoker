//! A model's action profile in a session: the schema it is shown names the verbs it was trained
//! on, and a drag from the cursor needs the step's cursor.

use crate::support;

use cua_action::{
    ActionProfile, CallFormat, Coord, CuaAction, CuaDialect, DragStyle, Point, ScrollAxis,
    ToolDialect, VerbAlias,
};
use cua_parse::DropReason;
use cua_session::{CuaSession, StepOutcome, TurnTranscript};
use model_provider::{ModelName, Part, StopReason, ToolSpec};
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
        ..ActionProfile::default()
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

fn read_by_us(calls: CallFormat) -> ActionProfile {
    ActionProfile { calls, ..ui_mate() }
}

/// A session with one repair, for a model whose profile says we read its calls.
fn reading(calls: CallFormat) -> CuaSession {
    let profile = profile(QWEN, grid(), 0, 1).with_actions(read_by_us(calls));
    CuaSession::begin(profile, task(), ModelName("ui-mate".into()))
}

fn xml_call(action: &str, extra: &str) -> String {
    format!(
        "<tool_call>\n<function=computer_use>\n<parameter=action>\n{action}\n</parameter>\n{extra}</function>\n</tool_call>"
    )
}

#[test]
fn a_model_whose_calls_we_read_is_told_its_tools_in_the_system_message_and_declares_none() {
    for calls in [CallFormat::Qwen3CoderXml, CallFormat::HermesJson] {
        let request = reading(calls).request(&obs(0), &map(grid()), frame("f"));
        assert!(
            request.tools.is_empty(),
            "{calls:?}: the server must not parse"
        );
        let Part::Text(system) = &request.messages[0].parts[0] else {
            panic!("a system text")
        };
        assert!(system.contains("# Tools"), "{system}");
        assert!(system.contains("<tools>\n{"), "{system}");
        assert!(system.contains("\"name\":\"computer_use\""), "{system}");
        // The definition is the schema the session builds, with the profile's verbs.
        assert!(system.contains("finished"), "{system}");
        assert_eq!(
            system.contains("<function=example_function_name>"),
            calls == CallFormat::Qwen3CoderXml
        );
    }
    // An engine-parsed model declares its tool and its system text has no tool block.
    let request = session_with(ui_mate()).request(&obs(0), &map(grid()), frame("f"));
    assert_eq!(request.tools.len(), 1);
    let Part::Text(system) = &request.messages[0].parts[0] else {
        panic!("a system text")
    };
    assert!(
        !system.contains("# Tools") && !system.contains("<tool_call>"),
        "{system}"
    );
}

#[test]
fn a_call_in_the_reply_text_becomes_an_action_as_a_server_parsed_one_does() {
    let at = "<parameter=coordinate>\n[500, 250]\n</parameter>\n";
    let reasoned = format!(
        "I should click it.\n</think>\n\n{}",
        xml_call("left_click", at)
    );
    let mut s = reading(CallFormat::Qwen3CoderXml);
    let m = map(grid());
    let sent = s.request(&obs(0), &m, frame("f"));
    let (actions, dropped) = match s.absorb_for(&sent, said(&reasoned), &m) {
        StepOutcome::Actions {
            actions, dropped, ..
        } => (actions, dropped),
        other => panic!("actions expected, got {other:?}"),
    };
    assert!(dropped.is_empty(), "{dropped:?}");
    assert!(
        matches!(&actions[..], [CuaAction::Click { .. }]),
        "{actions:?}"
    );
    // The same call as the engine would hand it over gives the same action.
    let mut engine = session_with(ui_mate());
    let sent = engine.request(&obs(0), &m, frame("f"));
    let StepOutcome::Actions { actions: same, .. } = engine.absorb_for(
        &sent,
        calls(&[r#"{"action":"left_click","coordinate":[500,250]}"#]),
        &m,
    ) else {
        panic!("actions")
    };
    assert_eq!(actions, same);
}

#[test]
fn calls_we_read_that_cannot_be_run_are_dropped_for_the_right_reason() {
    let m = map(grid());
    for (what, text, reason) in [
        (
            "an unknown function",
            "<tool_call><function=bogus></function></tool_call>".to_owned(),
            DropReason::UnsupportedVerb,
        ),
        (
            "a click with no point",
            xml_call("left_click", ""),
            DropReason::MissingArgument,
        ),
        (
            "a point that is not two numbers",
            xml_call("left_click", "<parameter=coordinate>\n[1]\n</parameter>\n"),
            DropReason::BadNumber,
        ),
        (
            "a verb that does not exist",
            xml_call("levitate", ""),
            DropReason::UnsupportedVerb,
        ),
    ] {
        // No repair left, so the refusal is the outcome.
        let mut s = session_with(read_by_us(CallFormat::Qwen3CoderXml));
        let sent = s.request(&obs(0), &m, frame("f"));
        let StepOutcome::Actions {
            actions, dropped, ..
        } = s.absorb_for(&sent, said(&text), &m)
        else {
            panic!("{what}: actions")
        };
        assert!(actions.is_empty(), "{what}");
        assert_eq!(
            dropped.iter().map(|d| d.reason).collect::<Vec<_>>(),
            [reason],
            "{what}"
        );
    }
}

#[test]
fn a_call_in_a_foreign_form_is_not_read_and_the_repair_says_how_to_write_one() {
    // What the model wrote in its reasoning in the run that stalled: an `<invoke>` block.
    let path = format!(
        "{}/../../fixtures/ui_mate/stray_invoke.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let stray = std::fs::read_to_string(path).unwrap();
    let m = map(grid());
    let mut s = reading(CallFormat::Qwen3CoderXml);
    let sent = s.request(&obs(0), &m, frame("f"));
    let reply = TurnTranscript::new(
        String::new(),
        stray.clone(),
        vec![],
        end(StopReason::EndTurn),
    );
    let StepOutcome::Repair(repair) = s.absorb_for(&sent, reply, &m) else {
        panic!("a repair")
    };
    assert!(repair.tools.is_empty());
    let Some(Part::Text(asked)) = repair.messages.last().and_then(|m| m.parts.last()) else {
        panic!("a repair text")
    };
    assert!(
        asked.contains("<tool_call>") && asked.contains("<function=computer_use>"),
        "{asked}"
    );
    assert!(asked.contains("did not hold an action"), "{asked}");
    assert!(
        !asked.contains("invoke"),
        "the repair never repeats the reply: {asked}"
    );
    // The same fault in a model the engine serves gets the plain message.
    let mut plain = CuaSession::begin(
        profile(QWEN, grid(), 0, 1).with_actions(ui_mate()),
        task(),
        ModelName("ui-mate".into()),
    );
    let sent = plain.request(&obs(0), &m, frame("f"));
    let StepOutcome::Repair(repair) = plain.absorb_for(&sent, said(&stray), &m) else {
        panic!("a repair")
    };
    let Some(Part::Text(plain_said)) = repair.messages.last().and_then(|m| m.parts.last()) else {
        panic!("a repair text")
    };
    assert!(!plain_said.contains("<tool_call>"), "{plain_said}");
}
