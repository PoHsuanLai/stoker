//! Tool-call dialects: Qwen's `computer_use` and Holo 3.1's schema.

mod common;

use std::collections::BTreeSet;

use common::*;
use cua_action::{
    Button, Choice, Chord, ClickCount, Coord, CuaAction, FinishOutcome, ImageSpace, Length,
    Modifier, Notches, Repeat, ScrollBy, ScrollDir, Summary, Target, ToolDialect, TypedText,
    WaitMs,
};
use cua_parse::{DropReason, ParseError};
use keyboard_types::Key;
use serde_json::Value;

const QWEN: ToolDialect = ToolDialect::QwenComputerUse;
const HOLO: ToolDialect = ToolDialect::Holo31;

fn qwen(arguments: &str) -> cua_parse::Parsed {
    tools(QWEN, &[call("computer_use", arguments)]).unwrap()
}

fn holo(name: &str, arguments: &str) -> cua_parse::Parsed {
    tools(HOLO, &[call(name, arguments)]).unwrap()
}

fn fixture_calls(dir: &str, name: &str) -> Vec<model_provider::ToolCall> {
    let path = format!("{}/../../fixtures/{dir}/{name}", env!("CARGO_MANIFEST_DIR"));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| call(c["name"].as_str().unwrap(), &c["arguments"].to_string()))
        .collect()
}

#[test]
fn qwen_tool_calls_parse() {
    let p = tools(QWEN, &fixture_calls("qwen_cu", "left_click.json")).unwrap();
    assert_eq!(image_actions(&p), [left_click(at(412, 288))]);

    let p = tools(QWEN, &fixture_calls("qwen_cu", "key_then_type.json")).unwrap();
    let chord = Chord::new(BTreeSet::from([Modifier::Ctrl]), Key::Character("a".into())).unwrap();
    assert_eq!(
        image_actions(&p),
        [
            CuaAction::Key {
                chord,
                repeat: Repeat::ONCE
            },
            CuaAction::Type {
                text: TypedText::new("hello").unwrap()
            },
        ]
    );

    let p = tools(QWEN, &fixture_calls("qwen_cu", "scroll_and_terminate.json")).unwrap();
    assert_eq!(
        image_actions(&p),
        [
            CuaAction::Scroll {
                at: at(600, 400),
                dir: ScrollDir::Down,
                by: ScrollBy::Distance(Length::<ImageSpace>::new(Coord(300))),
            },
            CuaAction::Finish {
                outcome: FinishOutcome::Done,
                summary: Summary::new("").unwrap(),
                extracted: vec![],
            },
        ]
    );
    assert_eq!(p.thought, None);
}

#[test]
fn holo_tool_calls_parse() {
    let p = tools(HOLO, &fixture_calls("holo_31", "click_and_ask.json")).unwrap();
    assert_eq!(
        image_actions(&p),
        [
            left_click(at(500, 250)),
            CuaAction::Ask {
                question: Summary::new("Which account?").unwrap(),
                choices: vec![
                    Choice::new("Work").unwrap(),
                    Choice::new("Personal").unwrap()
                ],
            },
        ]
    );
}

#[test]
fn qwen_click_family() {
    let click = |to, button, count| CuaAction::Click {
        at: to,
        button,
        count,
        mods: BTreeSet::new(),
    };
    let cases = [
        ("right_click", Button::Right, ClickCount::One),
        ("middle_click", Button::Middle, ClickCount::One),
        ("double_click", Button::Left, ClickCount::Two),
        ("triple_click", Button::Left, ClickCount::Three),
    ];
    for (action, button, count) in cases {
        let p = qwen(&format!(r#"{{"action":"{action}","coordinate":[7,9]}}"#));
        assert_eq!(
            image_actions(&p),
            [click(at(7, 9), button, count)],
            "{action}"
        );
    }
    let p = qwen(r#"{"action":"mouse_move","coordinate":[1,2]}"#);
    assert_eq!(image_actions(&p), [CuaAction::MoveTo { at: at(1, 2) }]);
    // Integral floats are numbers; fractions are not.
    assert_eq!(
        image_actions(&qwen(r#"{"action":"left_click","coordinate":[7.0,9]}"#)).len(),
        1
    );
    let p = qwen(r#"{"action":"left_click","coordinate":[7.5,9]}"#);
    assert_eq!(drops(&p), [("left_click", DropReason::BadNumber)]);
}

#[test]
fn qwen_drag_scroll_wait_and_conclusions() {
    let p = qwen(r#"{"action":"left_click_drag","start_coordinate":[1,2],"coordinate":[30,40]}"#);
    assert_eq!(
        image_actions(&p),
        [CuaAction::Drag {
            from: at(1, 2),
            to: at(30, 40),
            button: Button::Left
        }]
    );
    let p = qwen(r#"{"action":"left_click_drag","coordinate":[30,40]}"#);
    assert_eq!(
        drops(&p),
        [("left_click_drag", DropReason::MissingArgument)]
    );

    let scroll = |args: &str| image_actions(&qwen(args))[0].clone();
    let by = |dir, px| CuaAction::Scroll {
        at: at(5, 5),
        dir,
        by: ScrollBy::Distance(Length::new(Coord(px))),
    };
    assert_eq!(
        scroll(r#"{"action":"scroll","coordinate":[5,5],"pixels":120}"#),
        by(ScrollDir::Up, 120)
    );
    assert_eq!(
        scroll(r#"{"action":"scroll","coordinate":[5,5],"pixels":-120}"#),
        by(ScrollDir::Down, 120)
    );
    assert_eq!(
        scroll(r#"{"action":"hscroll","coordinate":[5,5],"pixels":50}"#),
        by(ScrollDir::Right, 50)
    );
    assert_eq!(
        scroll(r#"{"action":"hscroll","coordinate":[5,5],"pixels":-50}"#),
        by(ScrollDir::Left, 50)
    );
    let zero = qwen(r#"{"action":"scroll","coordinate":[5,5],"pixels":0}"#);
    assert_eq!(drops(&zero), [("scroll", DropReason::BadNumber)]);
    // Qwen's schema makes the coordinate optional: the scroll acts at the centre of the frame.
    let nowhere = qwen(r#"{"action":"scroll","pixels":10}"#);
    assert_eq!(
        image_actions(&nowhere),
        [CuaAction::Scroll {
            at: Target::Centre,
            dir: ScrollDir::Up,
            by: ScrollBy::Distance(Length::new(Coord(10))),
        }]
    );
    let half = qwen(r#"{"action":"scroll","coordinate":[1],"pixels":10}"#);
    assert_eq!(drops(&half), [("scroll", DropReason::BadNumber)]);
    let half = qwen(r#"{"action":"scroll","x":1,"pixels":10}"#);
    assert_eq!(drops(&half), [("scroll", DropReason::MissingArgument)]);

    let wait = |s: &str| qwen(&format!(r#"{{"action":"wait","time":{s}}}"#));
    assert_eq!(
        image_actions(&wait("1.5")),
        [CuaAction::Wait {
            for_ms: WaitMs::new(1500).unwrap()
        }]
    );
    assert_eq!(image_actions(&wait("60")).len(), 1);
    for bad in ["61", "-1", "1e400"] {
        let p = if bad == "1e400" {
            qwen(r#"{"action":"wait","time":"1e400"}"#)
        } else {
            wait(bad)
        };
        assert_eq!(drops(&p), [("wait", DropReason::BadNumber)], "{bad}");
    }

    let failed = qwen(r#"{"action":"terminate","status":"failure"}"#);
    let CuaAction::Finish { outcome, .. } = &image_actions(&failed)[0] else {
        panic!()
    };
    assert_eq!(*outcome, FinishOutcome::Failed);
    let answer = qwen(r#"{"action":"answer","text":"42"}"#);
    let CuaAction::Finish {
        outcome, summary, ..
    } = &image_actions(&answer)[0]
    else {
        panic!()
    };
    assert_eq!((*outcome, summary.as_str()), (FinishOutcome::Done, "42"));
    let bad = qwen(r#"{"action":"terminate","status":"maybe"}"#);
    assert_eq!(drops(&bad), [("terminate", DropReason::BadArgument)]);
}

#[test]
fn key_spellings_in_tool_calls() {
    let chord = |args: &str| match &image_actions(&qwen(args))[0] {
        CuaAction::Key { chord, .. } => chord.clone(),
        other => panic!("{other:?}"),
    };
    let want = Chord::new(BTreeSet::from([Modifier::Alt]), Key::Tab).unwrap();
    assert_eq!(chord(r#"{"action":"key","keys":["alt","tab"]}"#), want);
    assert_eq!(chord(r#"{"action":"key","keys":"Alt+Tab"}"#), want);
    let p = qwen(r#"{"action":"key","keys":[1,2]}"#);
    assert_eq!(drops(&p), [("key", DropReason::BadArgument)]);
    let p = qwen(r#"{"action":"key","keys":"a b"}"#);
    assert_eq!(drops(&p), [("key", DropReason::BadArgument)]);
    let p = qwen(r#"{"action":"key","keys":"ctrl+nonsense"}"#);
    assert_eq!(drops(&p), [("key", DropReason::BadArgument)]);
    let p = qwen(r#"{"action":"key"}"#);
    assert_eq!(drops(&p), [("key", DropReason::MissingArgument)]);
}

#[test]
fn unknown_verbs_and_shapes_are_dropped_not_guessed() {
    let p = qwen(r#"{"action":"open_app","name":"Files"}"#);
    assert_eq!(drops(&p), [("open_app", DropReason::UnsupportedVerb)]);
    let p = tools(QWEN, &[call("shell", r#"{"cmd":"rm -rf /"}"#)]).unwrap();
    assert_eq!(drops(&p), [("shell", DropReason::UnsupportedVerb)]);
    let p = holo("shell", r#"{"cmd":"ls"}"#);
    assert_eq!(drops(&p), [("shell", DropReason::UnsupportedVerb)]);
    // Holo's own verbs are not Qwen's.
    let p = tools(QWEN, &[call("click", r#"{"x":1,"y":2}"#)]).unwrap();
    assert_eq!(drops(&p), [("click", DropReason::UnsupportedVerb)]);
    let p = qwen(r#"{"action":"ask","question":"?"}"#);
    assert_eq!(drops(&p), [("ask", DropReason::UnsupportedVerb)]);
    let p = qwen(r#"{"coordinate":[1,2]}"#);
    assert_eq!(drops(&p), [("computer_use", DropReason::MissingArgument)]);
    for not_an_object in ["[1,2]", "7", "null", r#""click""#] {
        let p = qwen(not_an_object);
        assert_eq!(
            drops(&p),
            [("computer_use", DropReason::MissingArgument)],
            "{not_an_object}"
        );
    }
    assert_eq!(tools(QWEN, &[]), Err(ParseError::NoAction));
}

#[test]
fn holo_schema_variants() {
    assert_eq!(
        image_actions(&holo("click", r#"{"x":5,"y":6}"#)),
        [left_click(at(5, 6))]
    );
    assert_eq!(
        image_actions(&holo("click", r#"{"coordinate":[5,6]}"#)),
        [left_click(at(5, 6))]
    );
    // The Qwen function works under Holo too.
    assert_eq!(
        image_actions(&holo(
            "computer_use",
            r#"{"action":"left_click","coordinate":[5,6]}"#
        )),
        [left_click(at(5, 6))]
    );
    let p = holo("drag", r#"{"start_x":1,"start_y":2,"end_x":3,"end_y":4}"#);
    assert_eq!(
        image_actions(&p),
        [CuaAction::Drag {
            from: at(1, 2),
            to: at(3, 4),
            button: Button::Left
        }]
    );
    let p = holo("scroll", r#"{"x":9,"y":9,"direction":"Left"}"#);
    assert_eq!(
        image_actions(&p),
        [CuaAction::Scroll {
            at: at(9, 9),
            dir: ScrollDir::Left,
            by: ScrollBy::Notches(Notches(5))
        }]
    );
    assert_eq!(image_actions(&holo("observe", "{}")), [CuaAction::Observe]);
    let p = holo(
        "finish",
        r#"{"status":"infeasible","summary":"no such menu"}"#,
    );
    let CuaAction::Finish {
        outcome, summary, ..
    } = &image_actions(&p)[0]
    else {
        panic!()
    };
    assert_eq!(
        (*outcome, summary.as_str()),
        (FinishOutcome::Infeasible, "no such menu")
    );
    // One coordinate alone is not a point.
    assert_eq!(
        drops(&holo("click", r#"{"x":5}"#)),
        [("click", DropReason::MissingArgument)]
    );
    let many: Vec<String> = (0..9).map(|i| format!("\"c{i}\"")).collect();
    let args = format!(r#"{{"question":"q","choices":[{}]}}"#, many.join(","));
    assert_eq!(drops(&holo("ask", &args)), [("ask", DropReason::TooLong)]);
}

#[test]
fn grid_space_checks_the_top() {
    use cua_action::{GridMax, ModelSpace};
    let space = ModelSpace::Grid(GridMax(1000));
    let calls = [
        call(
            "computer_use",
            r#"{"action":"left_click","coordinate":[1000,10]}"#,
        ),
        call(
            "computer_use",
            r#"{"action":"left_click","coordinate":[1001,10]}"#,
        ),
    ];
    let p = cua_parse::parse_tool_calls(QWEN, space, &calls, Default::default()).unwrap();
    assert_eq!(grid_actions(&p).len(), 1);
    assert_eq!(drops(&p), [("left_click", DropReason::OutOfFrame)]);
}

#[test]
fn deeply_nested_arguments_do_not_overflow() {
    let deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
    let args = format!(r#"{{"action":"left_click","coordinate":{deep}}}"#);
    // JsonText accepts only what serde_json can read; if it refuses, the boundary did its job.
    if let Ok(input) = model_provider::JsonText::new(args) {
        let c = model_provider::ToolCall {
            id: model_provider::ToolCallId("x".into()),
            name: model_provider::ToolName::new("computer_use").unwrap(),
            input,
        };
        let p = tools(QWEN, &[c]).unwrap();
        assert_eq!(p.dropped.len(), 1);
    }
}
