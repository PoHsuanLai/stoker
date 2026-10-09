//! The decoders, over examples written from each vendor's documentation (2026-10-03): the
//! Anthropic toolset and `computer_20251124`, OpenAI's `computer_call`, Gemini's computer use.

use cua_action::{
    Button, ClickCount, Coord, CuaAction, GridMax, GridSpace, ImageSpace, Modifier, Notches,
    ScrollBy, ScrollDir, Target, WireDialect,
};
use cua_parse::{DropReason, InSpace, Parsed};
use cua_vendors::{WireCodec, WireError, codec};
use model_provider::{JsonText, SafetySignal, ToolCall, ToolCallId, ToolName};
use proptest::prelude::*;

fn call(name: &str, input: &str) -> ToolCall {
    ToolCall {
        id: ToolCallId("c".into()),
        name: ToolName::new(name).unwrap(),
        input: JsonText::new(input).unwrap(),
    }
}

fn decode(dialect: WireDialect, calls: &[ToolCall]) -> Result<Parsed, WireError> {
    codec(dialect).decode(calls, &[])
}

fn image(parsed: Parsed) -> (Vec<CuaAction<ImageSpace>>, Vec<(String, DropReason)>) {
    let InSpace::Image(actions) = parsed.actions else {
        panic!("image space")
    };
    let dropped = parsed
        .dropped
        .into_iter()
        .map(|d| (d.verb.as_str().to_owned(), d.reason))
        .collect();
    (actions, dropped)
}

fn point(x: u32, y: u32) -> Target<ImageSpace> {
    Target::Point(cua_action::Point::new(Coord(x), Coord(y)))
}

const TOOLSET: WireDialect = WireDialect::AnthropicToolset20260801;
const LEGACY: WireDialect = WireDialect::AnthropicComputer20251124;

#[test]
fn the_toolset_members_decode() {
    let calls = [
        call("left_click", r#"{"coordinate":[500,300],"text":"shift"}"#),
        call(
            "left_click_drag",
            r#"{"start_coordinate":[200,300],"coordinate":[600,300]}"#,
        ),
        call(
            "scroll",
            r#"{"coordinate":[500,400],"scroll_direction":"down","scroll_amount":3}"#,
        ),
        call("zoom", r#"{"region":[100,200,400,350]}"#),
        call("type", r#"{"text":"search query"}"#),
        call("key", r#"{"text":"ctrl+s"}"#),
        call("wait", r#"{"duration":2}"#),
        call("screenshot", "{}"),
    ];
    let (actions, dropped) = image(decode(TOOLSET, &calls).unwrap());
    assert_eq!(dropped, vec![]);
    let kinds: Vec<&str> = actions
        .iter()
        .map(|a| match a {
            CuaAction::Click { .. } => "click",
            CuaAction::Drag { .. } => "drag",
            CuaAction::Scroll { .. } => "scroll",
            CuaAction::Zoom { .. } => "zoom",
            CuaAction::Type { .. } => "type",
            CuaAction::Key { .. } => "key",
            CuaAction::Wait { .. } => "wait",
            CuaAction::Observe => "observe",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "click", "drag", "scroll", "zoom", "type", "key", "wait", "observe"
        ]
    );
    assert_eq!(
        actions[0],
        CuaAction::Click {
            at: point(500, 300),
            button: Button::Left,
            count: ClickCount::One,
            mods: [Modifier::Shift].into(),
        }
    );
    let CuaAction::Scroll { at, dir, by } = &actions[2] else {
        unreachable!()
    };
    assert_eq!(
        (at, dir, by),
        (
            &point(500, 400),
            &ScrollDir::Down,
            &ScrollBy::Notches(Notches(3))
        )
    );
    let CuaAction::Zoom { region } = &actions[3] else {
        unreachable!()
    };
    assert_eq!(
        (region.origin.x.0, region.size.w.0, region.size.h.0),
        (100, 300, 150)
    );
    let CuaAction::Wait { for_ms } = &actions[6] else {
        unreachable!()
    };
    assert_eq!(for_ms.ms(), 2000);
}

#[test]
fn the_2025_tool_names_the_member_in_the_input() {
    let calls = [
        call(
            "computer",
            r#"{"action":"double_click","coordinate":[10,20]}"#,
        ),
        call(
            "computer",
            r#"{"action":"key","text":"alt+Tab","repeat":2}"#,
        ),
        call("computer", r#"{"action":"screenshot"}"#),
    ];
    let (actions, dropped) = image(decode(LEGACY, &calls).unwrap());
    assert_eq!(dropped, vec![]);
    assert!(matches!(
        &actions[0],
        CuaAction::Click {
            count: ClickCount::Two,
            ..
        }
    ));
    let CuaAction::Key { repeat, .. } = &actions[1] else {
        unreachable!()
    };
    assert_eq!(repeat.times(), 2);
    assert_eq!(actions[2], CuaAction::Observe);
    // The toolset's names are not this tool's.
    assert_eq!(
        decode(LEGACY, &[call("left_click", r#"{"coordinate":[1,1]}"#)]),
        Err(WireError::UnknownTool)
    );
}

#[test]
fn what_we_do_not_act_on_or_cannot_place_is_dropped_with_its_reason() {
    use DropReason::*;
    let cases: &[(&str, &str, DropReason)] = &[
        ("left_mouse_down", "{}", UnsupportedVerb),
        ("cursor_position", "{}", UnsupportedVerb),
        (
            "hold_key",
            r#"{"text":"shift","duration":2}"#,
            UnsupportedVerb,
        ),
        ("left_click", "{}", MissingArgument),
        ("left_click", r#"{"coordinate":[1]}"#, BadArgument),
        ("left_click", r#"{"coordinate":[-1,2]}"#, BadNumber),
        (
            "left_click",
            r#"{"coordinate":[1,2],"text":"hyper"}"#,
            BadArgument,
        ),
        (
            "scroll",
            r#"{"scroll_direction":"down","scroll_amount":1}"#,
            MissingArgument,
        ),
        (
            "scroll",
            r#"{"coordinate":[1,1],"scroll_direction":"sideways","scroll_amount":1}"#,
            BadArgument,
        ),
        (
            "scroll",
            r#"{"coordinate":[1,1],"scroll_direction":"up","scroll_amount":0}"#,
            BadNumber,
        ),
        ("type", r#"{"text":""}"#, BadArgument),
        ("key", r#"{"text":"a+b"}"#, BadArgument),
        ("key", r#"{"text":"ctrl+s","repeat":21}"#, BadNumber),
        ("wait", r#"{"duration":301}"#, BadNumber),
        ("zoom", r#"{"region":[10,10,10,20]}"#, BadNumber),
        ("zoom", r#"{"region":[1,2,3]}"#, BadArgument),
    ];
    for (name, input, reason) in cases {
        // One good call beside it, so the reply as a whole still decodes.
        let calls = [call(name, input), call("screenshot", "{}")];
        let (actions, dropped) = image(decode(TOOLSET, &calls).unwrap());
        assert_eq!(actions, vec![CuaAction::Observe], "{name} {input}");
        assert_eq!(
            dropped,
            vec![((*name).to_owned(), *reason)],
            "{name} {input}"
        );
    }
}

#[test]
fn a_reply_that_is_no_tool_of_the_dialect_is_a_fault_of_the_reply() {
    assert_eq!(
        decode(TOOLSET, &[call("bash", "{}")]),
        Err(WireError::UnknownTool)
    );
    let not_an_object = call("left_click", "[1,2]");
    assert_eq!(
        decode(TOOLSET, &[not_an_object]),
        Err(WireError::BadArguments)
    );
    assert_eq!(decode(TOOLSET, &[]).unwrap().dropped, vec![]);
}

#[test]
fn more_than_the_batch_limit_is_dropped() {
    let calls: Vec<ToolCall> = (0..9).map(|_| call("screenshot", "{}")).collect();
    let (actions, dropped) = image(decode(TOOLSET, &calls).unwrap());
    assert_eq!(actions.len(), 8);
    assert_eq!(
        dropped,
        vec![("screenshot".to_owned(), DropReason::OverBatchLimit)]
    );
}

#[test]
fn openai_decodes_the_actions_of_a_computer_call() {
    let input = r#"{"actions":[
        {"type":"click","button":"left","x":405,"y":157},
        {"type":"type","text":"penguin"},
        {"type":"keypress","keys":["CTRL","A"]},
        {"type":"scroll","x":10,"y":20,"scroll_x":0,"scroll_y":-300},
        {"type":"drag","path":[{"x":1,"y":2},{"x":5,"y":6},{"x":9,"y":10}]},
        {"type":"wait"},{"type":"screenshot"},{"type":"move","x":3,"y":4},
        {"type":"double_click","x":7,"y":8}
    ]}"#;
    let calls = [call("computer", input)];
    let (actions, dropped) = image(decode(WireDialect::OpenAiComputer, &calls).unwrap());
    assert_eq!(
        dropped,
        vec![("double_click".to_owned(), DropReason::OverBatchLimit)]
    );
    assert_eq!(
        actions[0],
        CuaAction::Click {
            at: point(405, 157),
            button: Button::Left,
            count: ClickCount::One,
            mods: Default::default()
        }
    );
    let CuaAction::Scroll {
        dir,
        by: ScrollBy::Distance(len),
        ..
    } = &actions[3]
    else {
        panic!("scroll by distance")
    };
    assert_eq!((*dir, len.coord()), (ScrollDir::Up, Coord(300)));
    assert!(
        matches!(&actions[4], CuaAction::Drag { from, to, .. } if *from == point(1, 2) && *to == point(9, 10))
    );
    assert!(matches!(&actions[5], CuaAction::Wait { for_ms } if for_ms.ms() == 2000));
    // The older single-action form, and the verbs that do not decode.
    let single = [call(
        "computer",
        r#"{"action":{"type":"click","button":"back","x":1,"y":2}}"#,
    )];
    let (actions, dropped) = image(decode(WireDialect::OpenAiComputer, &single).unwrap());
    assert!(actions.is_empty());
    assert_eq!(dropped, vec![("click".to_owned(), DropReason::BadArgument)]);
    assert_eq!(
        decode(WireDialect::OpenAiComputer, &[call("other", "{}")]),
        Err(WireError::UnknownTool)
    );
}

#[test]
fn gemini_points_are_a_grid_of_one_thousand() {
    let calls = [
        call("click", r#"{"x":500,"y":250,"intent":"open it"}"#),
        call("type", r#"{"text":"hi","press_enter":true}"#),
        call(
            "scroll",
            r#"{"x":10,"y":10,"direction":"down","magnitude_in_pixels":250}"#,
        ),
        call(
            "drag_and_drop",
            r#"{"start_x":1,"start_y":2,"end_x":3,"end_y":4}"#,
        ),
        call("hotkey", r#"{"keys":["ctrl","c"]}"#),
        call("navigate", r#"{"url":"https://example.com"}"#),
        call("click", r#"{"x":1001,"y":5}"#),
    ];
    let parsed = decode(WireDialect::GeminiComputerUse, &calls).unwrap();
    let InSpace::Grid(max, actions) = parsed.actions else {
        panic!("grid space")
    };
    assert_eq!(max, GridMax(1000));
    let at: Target<GridSpace> = Target::Point(cua_action::Point::new(Coord(500), Coord(250)));
    assert!(matches!(&actions[0], CuaAction::Click { at: a, .. } if *a == at));
    let CuaAction::Type { text } = &actions[1] else {
        unreachable!()
    };
    assert_eq!(text.as_str(), "hi\n");
    assert!(matches!(
        &actions[2],
        CuaAction::Scroll {
            by: ScrollBy::Notches(Notches(3)),
            ..
        }
    ));
    assert_eq!(actions.len(), 5);
    let dropped: Vec<(String, DropReason)> = parsed
        .dropped
        .iter()
        .map(|d| (d.verb.as_str().to_owned(), d.reason))
        .collect();
    assert_eq!(
        dropped,
        vec![
            ("navigate".to_owned(), DropReason::UnsupportedVerb),
            ("click".to_owned(), DropReason::OutOfFrame)
        ]
    );
}

#[test]
fn gemini_safety_only_adds() {
    let click = [call("click", r#"{"x":5,"y":5}"#)];
    let gemini = codec(WireDialect::GeminiComputerUse);
    let plain = gemini.decode(&click, &[]).unwrap();
    let InSpace::Grid(_, plain_actions) = plain.actions.clone() else {
        panic!()
    };
    // A confirmation request puts an ask ahead of the actions and takes nothing away.
    let asked = gemini
        .decode(
            &click,
            &[SafetySignal::RequireConfirmation("a purchase".into())],
        )
        .unwrap();
    let InSpace::Grid(_, actions) = asked.actions else {
        panic!()
    };
    assert_eq!(actions.len(), plain_actions.len() + 1);
    assert!(matches!(&actions[0], CuaAction::Ask { .. }));
    assert_eq!(&actions[1..], &plain_actions[..]);
    // A block removes the actions and ends the run as infeasible.
    let blocked = gemini
        .decode(&click, &[SafetySignal::Blocked("not allowed".into())])
        .unwrap();
    let InSpace::Grid(_, actions) = blocked.actions else {
        panic!()
    };
    assert!(matches!(
        &actions[..],
        [CuaAction::Finish {
            outcome: cua_action::FinishOutcome::Infeasible,
            ..
        }]
    ));
    // Hints never make an action more permitted: no hint means the plain decode.
    assert_eq!(gemini.decode(&click, &[]).unwrap(), plain);
}

proptest! {
    #[test]
    fn decoding_never_panics(
        name in "[a-z_]{1,16}",
        input in "[\\[\\]{}\":,a-z0-9 .+_-]{0,100}",
    ) {
        for dialect in [TOOLSET, LEGACY, WireDialect::OpenAiComputer, WireDialect::GeminiComputerUse] {
            if let (Ok(tool), Ok(json)) = (ToolName::new(name.clone()), JsonText::new(input.clone())) {
                let calls = [ToolCall { id: ToolCallId("c".into()), name: tool, input: json }];
                let _ = codec(dialect).decode(&calls, &[]);
            }
        }
    }
}
