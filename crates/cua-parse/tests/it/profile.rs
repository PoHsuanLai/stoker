//! Action profiles: a model that speaks a variant of the common computer-use tool (UI-Mate:
//! renamed verbs, a drag from the cursor, a scroll whose axis is an argument) is read through
//! the profile its catalogue entry states, and an entry without one is read as before.

use crate::common;

use std::collections::BTreeSet;

use common::*;
use cua_action::{
    ActionProfile, Button, Chord, ClickCount, Coord, CuaAction, DragStyle, FinishOutcome, GridMax,
    GridSpace, Length, ModelSpace, Modifier, Repeat, ScrollAxis, ScrollBy, ScrollDir, Summary,
    Target, ToolDialect, VerbAlias, WaitMs,
};
use cua_parse::{CursorAt, DropReason, Parsed, ToolContext, parse_tool_calls_with};
use keyboard_types::Key;

const QWEN: ToolDialect = ToolDialect::QwenComputerUse;
const CURSOR: CursorAt = CursorAt { x: 100, y: 200 };

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

fn read(profile: ActionProfile, cursor: Option<CursorAt>, arguments: &str) -> Parsed {
    let context = ToolContext { profile, cursor };
    let calls = [call("computer_use", arguments)];
    let space = ModelSpace::Grid(GridMax(999));
    parse_tool_calls_with(QWEN, context, space, &calls, Default::default()).unwrap()
}

fn ctrl_c() -> CuaAction<GridSpace> {
    let chord = Chord::new(BTreeSet::from([Modifier::Ctrl]), Key::Character("c".into())).unwrap();
    CuaAction::Key {
        chord,
        repeat: Repeat::ONCE,
    }
}

fn finish(outcome: FinishOutcome) -> CuaAction<GridSpace> {
    CuaAction::Finish {
        outcome,
        summary: Summary::new("").unwrap(),
        extracted: vec![],
    }
}

fn scroll(dir: ScrollDir, pixels: u32) -> CuaAction<GridSpace> {
    CuaAction::Scroll {
        at: Target::Centre,
        dir,
        by: ScrollBy::Distance(Length::<GridSpace>::new(Coord(pixels))),
    }
}

#[test]
fn every_ui_mate_verb_reads_through_its_profile() {
    let click = |x, y, button, count| CuaAction::Click {
        at: gat(x, y),
        button,
        count,
        mods: BTreeSet::new(),
    };
    let cases: Vec<(&str, CuaAction<GridSpace>)> = vec![
        (
            r#"{"action":"left_click","coordinate":[10,20]}"#,
            click(10, 20, Button::Left, ClickCount::One),
        ),
        (
            r#"{"action":"triple_click","coordinate":[999,0]}"#,
            click(999, 0, Button::Left, ClickCount::Three),
        ),
        (
            r#"{"action":"mouse_move","coordinate":[5,6]}"#,
            CuaAction::MoveTo { at: gat(5, 6) },
        ),
        (
            r#"{"action":"drag","coordinate":[300,400]}"#,
            CuaAction::Drag {
                from: gat(100, 200),
                to: gat(300, 400),
                button: Button::Left,
            },
        ),
        (r#"{"action":"hotkey","keys":["ctrl","c"]}"#, ctrl_c()),
        (
            r#"{"action":"press","keys":["escape"]}"#,
            CuaAction::Key {
                chord: Chord::new(BTreeSet::new(), Key::Escape).unwrap(),
                repeat: Repeat::ONCE,
            },
        ),
        (
            r#"{"action":"wait","time":1.5}"#,
            CuaAction::Wait {
                for_ms: WaitMs::new(1500).unwrap(),
            },
        ),
        (
            r#"{"action":"finished","status":"success"}"#,
            finish(FinishOutcome::Done),
        ),
        (
            r#"{"action":"finished","status":"failure"}"#,
            finish(FinishOutcome::Failed),
        ),
        (
            r#"{"action":"call_user","text":"Which account?"}"#,
            CuaAction::Ask {
                question: Summary::new("Which account?").unwrap(),
                choices: vec![],
            },
        ),
        (
            r#"{"action":"scroll","pixels":300}"#,
            scroll(ScrollDir::Up, 300),
        ),
        (
            r#"{"action":"scroll","pixels":-300,"direction":"vertical"}"#,
            scroll(ScrollDir::Down, 300),
        ),
        (
            r#"{"action":"scroll","pixels":-300,"direction":"horizontal"}"#,
            scroll(ScrollDir::Left, 300),
        ),
    ];
    for (arguments, expected) in cases {
        let p = read(ui_mate(), Some(CURSOR), arguments);
        assert_eq!(grid_actions(&p), [expected], "{arguments}");
        assert!(p.dropped.is_empty(), "{arguments}");
    }
}

#[test]
fn a_drag_from_the_cursor_needs_to_know_the_cursor() {
    let p = read(
        ui_mate(),
        None,
        r#"{"action":"drag","coordinate":[300,400]}"#,
    );
    assert!(grid_actions(&p).is_empty());
    assert_eq!(drops(&p), [("drag", DropReason::MissingArgument)]);
    // A cursor past the grid's top is refused, never clamped.
    let far = Some(CursorAt { x: 1000, y: 0 });
    let p = read(ui_mate(), far, r#"{"action":"drag","coordinate":[3,4]}"#);
    assert_eq!(drops(&p), [("drag", DropReason::OutOfFrame)]);
    // And the end of the drag is still needed.
    let p = read(ui_mate(), Some(CURSOR), r#"{"action":"drag"}"#);
    assert_eq!(drops(&p), [("drag", DropReason::MissingArgument)]);
}

#[test]
fn a_drag_after_a_move_in_the_same_reply_starts_where_the_move_ended() {
    // UI-Mate writes "move there, then drag" as one reply: the drag starts at the move, not at
    // the cursor the step began with.
    let context = ToolContext {
        profile: ui_mate(),
        cursor: Some(CURSOR),
    };
    let calls = [
        call(
            "computer_use",
            r#"{"action":"mouse_move","coordinate":[500,450]}"#,
        ),
        call(
            "computer_use",
            r#"{"action":"drag","coordinate":[700,600]}"#,
        ),
        call(
            "computer_use",
            r#"{"action":"drag","coordinate":[800,650]}"#,
        ),
    ];
    let space = ModelSpace::Grid(GridMax(999));
    let p = parse_tool_calls_with(QWEN, context, space, &calls, Default::default()).unwrap();
    assert_eq!(
        grid_actions(&p)[1..],
        [
            CuaAction::Drag {
                from: gat(500, 450),
                to: gat(700, 600),
                button: Button::Left,
            },
            CuaAction::Drag {
                from: gat(700, 600),
                to: gat(800, 650),
                button: Button::Left,
            },
        ]
    );
}

#[test]
fn what_the_profile_does_not_state_is_refused_as_before() {
    for (arguments, verb, reason) in [
        // Holding a key down cannot be said in an action: dropped, not guessed.
        (
            r#"{"action":"key_down","keys":["shift"]}"#,
            "key_down",
            DropReason::UnsupportedVerb,
        ),
        (
            r#"{"action":"key_up","keys":["shift"]}"#,
            "key_up",
            DropReason::UnsupportedVerb,
        ),
        (
            r#"{"action":"finished","status":"maybe"}"#,
            "finished",
            DropReason::BadArgument,
        ),
        (
            r#"{"action":"scroll","pixels":3,"direction":"up"}"#,
            "scroll",
            DropReason::BadArgument,
        ),
        (
            r#"{"action":"call_user"}"#,
            "call_user",
            DropReason::MissingArgument,
        ),
    ] {
        let p = read(ui_mate(), Some(CURSOR), arguments);
        assert_eq!(drops(&p), [(verb, reason)], "{arguments}");
    }
}

#[test]
fn an_entry_without_a_profile_parses_exactly_as_before() {
    // The verbs of a profile are unknown without it, and the common ones read as always.
    for verb in ["drag", "hotkey", "press", "finished", "call_user"] {
        let arguments = format!(r#"{{"action":"{verb}","coordinate":[3,4],"text":"x"}}"#);
        let p = read(ActionProfile::default(), Some(CURSOR), &arguments);
        assert_eq!(drops(&p), [(verb, DropReason::UnsupportedVerb)]);
    }
    let drag = r#"{"action":"left_click_drag","start_coordinate":[1,2],"coordinate":[3,4]}"#;
    let p = read(ActionProfile::default(), Some(CURSOR), drag);
    assert_eq!(
        grid_actions(&p),
        [CuaAction::Drag {
            from: gat(1, 2),
            to: gat(3, 4),
            button: Button::Left
        }]
    );
    // The default context is what `parse_tool_calls` uses: the same answer either way.
    let calls = [call("computer_use", drag)];
    let space = ModelSpace::Grid(GridMax(999));
    let plain = cua_parse::parse_tool_calls(QWEN, space, &calls, Default::default());
    let with = parse_tool_calls_with(
        QWEN,
        ToolContext::default(),
        space,
        &calls,
        Default::default(),
    );
    assert_eq!(plain, with);
}
