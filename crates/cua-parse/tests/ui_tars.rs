//! The UI-TARS-1.5 grammar: the model-card examples, escapes, boxes and refusals.

mod common;

use std::collections::BTreeSet;

use common::*;
use cua_action::{
    Button, Chord, ClickCount, CuaAction, FinishOutcome, Modifier, Notches, Repeat, ScrollBy,
    ScrollDir, Summary, TypedText, WaitMs,
};
use cua_parse::{ByteOffset, DropReason, ParseError};
use keyboard_types::Key;

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../fixtures/ui_tars_15/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn ui_tars_examples_parse() {
    let p = ui_tars(&fixture("click_box_tags.txt")).unwrap();
    assert_eq!(
        p.thought.as_deref(),
        Some("Click on the search bar at the top of the page.")
    );
    assert_eq!(image_actions(&p), [left_click(at(459, 203))]);
    assert!(p.dropped.is_empty());

    let p = ui_tars(&fixture("hotkey.txt")).unwrap();
    let chord = Chord::new(BTreeSet::from([Modifier::Ctrl]), Key::Character("c".into())).unwrap();
    assert_eq!(
        image_actions(&p),
        [CuaAction::Key {
            chord,
            repeat: Repeat::ONCE
        }]
    );

    let p = ui_tars(&fixture("type_escaped.txt")).unwrap();
    let text = TypedText::new("it's \"quoted\"\nsecond line").unwrap();
    assert_eq!(image_actions(&p), [CuaAction::Type { text }]);

    let p = ui_tars(&fixture("scroll_then_wait.txt")).unwrap();
    assert_eq!(
        image_actions(&p),
        [
            CuaAction::Scroll {
                at: at(500, 500),
                dir: ScrollDir::Down,
                by: ScrollBy::Notches(Notches(5)),
            },
            CuaAction::Wait {
                for_ms: WaitMs::new(5000).unwrap()
            },
        ]
    );

    let p = ui_tars(&fixture("finished.txt")).unwrap();
    assert_eq!(
        image_actions(&p),
        [CuaAction::Finish {
            outcome: FinishOutcome::Done,
            summary: Summary::new("The file is saved.").unwrap(),
            extracted: vec![],
        }]
    );
}

#[test]
fn click_family_and_drag() {
    let p = ui_tars(
        "Thought: t\nAction: left_double(start_box='(1,2)')\n\nright_single(start_box='(3,4)')\n\ndrag(start_box='(5,6)', end_box='(7,8)')",
    )
    .unwrap();
    let click = |to, button, count| CuaAction::Click {
        at: to,
        button,
        count,
        mods: BTreeSet::new(),
    };
    assert_eq!(
        image_actions(&p),
        [
            click(at(1, 2), Button::Left, ClickCount::Two),
            click(at(3, 4), Button::Right, ClickCount::One),
            CuaAction::Drag {
                from: at(5, 6),
                to: at(7, 8),
                button: Button::Left
            },
        ]
    );
}

#[test]
fn boxes_take_their_centre_and_tags_are_skipped() {
    for text in [
        "Action: click(start_box='(10,20,30,40)')",
        "Action: click(start_box='[10, 20, 30, 40]')",
        "Action: click(start_box='<bbox>10 20 30 40</bbox>')",
        "Action: click(point='<point>20 30</point>')",
        "Action: click(start_box=\"(20,30)\")",
        "Action: click(start_box=(20,30))",
    ] {
        assert_eq!(
            image_actions(&ui_tars(text).expect(text)),
            [left_click(at(20, 30))],
            "{text}"
        );
    }
}

#[test]
fn thought_is_optional_and_the_last_action_line_counts() {
    let p = ui_tars("Action: wait()").unwrap();
    assert_eq!(p.thought, None);
    // A thought that mentions the marker mid-line does not split the reply there.
    let p = ui_tars("Thought: the next Action: is to click\nAction: wait()").unwrap();
    assert_eq!(p.thought.as_deref(), Some("the next Action: is to click"));
    assert_eq!(image_actions(&p).len(), 1);
}

#[test]
fn hotkey_spellings() {
    let key = |text: &str| match &image_actions(&ui_tars(text).unwrap())[0] {
        CuaAction::Key { chord, .. } => (chord.mods.clone(), chord.key.clone()),
        other => panic!("{other:?}"),
    };
    let ctrl_shift_t = (
        BTreeSet::from([Modifier::Ctrl, Modifier::Shift]),
        Key::Character("t".into()),
    );
    assert_eq!(key("Action: hotkey(key='ctrl shift t')"), ctrl_shift_t);
    assert_eq!(key("Action: hotkey(key='ctrl+shift+t')"), ctrl_shift_t);
    assert_eq!(
        key("Action: hotkey(key='enter')"),
        (BTreeSet::new(), Key::Enter)
    );
    assert_eq!(
        key("Action: hotkey(key='alt Tab')"),
        (BTreeSet::from([Modifier::Alt]), Key::Tab)
    );
    assert_eq!(
        key("Action: hotkey(key='cmd up')"),
        (BTreeSet::from([Modifier::Super]), Key::ArrowUp)
    );
    assert_eq!(key("Action: hotkey(key='f5')"), (BTreeSet::new(), Key::F5));
    assert_eq!(
        key("Action: hotkey(key='super')"),
        (BTreeSet::new(), Key::Super)
    );
    assert_eq!(
        key("Action: hotkey(key='ctrl +')"),
        (BTreeSet::from([Modifier::Ctrl]), Key::Character("+".into()))
    );
}

#[test]
fn unknown_verbs_are_dropped_not_guessed() {
    let p = ui_tars("Thought: t\nAction: open_app(app_name='Files')\n\nclick(start_box='(1,2)')")
        .unwrap();
    assert_eq!(drops(&p), [("open_app", DropReason::UnsupportedVerb)]);
    assert_eq!(image_actions(&p), [left_click(at(1, 2))]);
    for verb in ["press_home", "long_press", "mouse_down", "navigate"] {
        let p = ui_tars(&format!("Action: {verb}(x='1')")).unwrap();
        assert_eq!(drops(&p), [(verb, DropReason::UnsupportedVerb)]);
        assert!(image_actions(&p).is_empty());
    }
}

#[test]
fn refused_arguments_are_dropped_with_a_reason() {
    let cases = [
        ("click()", DropReason::MissingArgument),
        ("click(start_box='')", DropReason::MissingArgument),
        ("click(start_box='(-5,3)')", DropReason::BadNumber),
        ("click(start_box='(1.5,3)')", DropReason::BadNumber),
        ("click(start_box='(99999999999,3)')", DropReason::BadNumber),
        ("click(start_box='(1,2,3)')", DropReason::BadNumber),
        (
            "scroll(start_box='(1,2)', direction='sideways')",
            DropReason::MissingArgument,
        ),
        ("scroll(start_box='(1,2)')", DropReason::MissingArgument),
        ("hotkey(key='')", DropReason::MissingArgument),
        ("hotkey(key='ctrl a b')", DropReason::MissingArgument),
        ("type(content='')", DropReason::MissingArgument),
        ("type()", DropReason::MissingArgument),
    ];
    for (call, reason) in cases {
        let p = ui_tars(&format!("Action: {call}")).unwrap();
        let verb = call.split('(').next().unwrap();
        assert_eq!(drops(&p), [(verb, reason)], "{call}");
    }
    let long = format!("Action: type(content='{}')", "a".repeat(2049));
    assert_eq!(
        drops(&ui_tars(&long).unwrap()),
        [("type", DropReason::TooLong)]
    );
}

#[test]
fn grid_points_past_the_top_are_refused_never_clamped() {
    let p =
        ui_tars_grid("Action: click(start_box='(1000,0)')\n\nclick(start_box='(1001,5)')").unwrap();
    assert_eq!(grid_actions(&p).len(), 1);
    assert_eq!(drops(&p), [("click", DropReason::OutOfFrame)]);
    assert_eq!(
        grid_actions(&p)[0],
        CuaAction::Click {
            at: gat(1000, 0),
            button: Button::Left,
            count: ClickCount::One,
            mods: BTreeSet::new()
        }
    );
    // The same text in image space has no top to refuse at.
    assert_eq!(
        image_actions(&ui_tars("Action: click(start_box='(1001,5)')").unwrap()).len(),
        1
    );
}

#[test]
fn replies_without_an_action_or_with_a_broken_one() {
    assert_eq!(ui_tars(""), Err(ParseError::NoAction));
    assert_eq!(ui_tars("Thought: hmm"), Err(ParseError::NoAction));
    assert_eq!(ui_tars("Action:"), Err(ParseError::NoAction));
    assert_eq!(
        ui_tars("Action: click(start_box='(1,2)'"),
        Err(ParseError::Unterminated)
    );
    assert_eq!(
        ui_tars("Action: type(content='abc"),
        Err(ParseError::Unterminated)
    );
    assert_eq!(
        ui_tars("Action: click(start_box="),
        Err(ParseError::Unterminated)
    );
    assert_eq!(
        ui_tars("Action: 12345"),
        Err(ParseError::Malformed { at: ByteOffset(8) })
    );
    assert_eq!(
        ui_tars("Action: click start_box"),
        Err(ParseError::Malformed { at: ByteOffset(14) })
    );
    // A broken call after a good one stays an error: a truncated reply is not trusted.
    assert_eq!(
        ui_tars("Action: wait()\n\nclick(start_box='(1,2)'"),
        Err(ParseError::Unterminated)
    );
    // Trailing end-of-turn text after a good call is ignored.
    assert!(ui_tars("Action: wait()<|im_end|>").is_ok());
}

#[test]
fn multibyte_text_is_safe() {
    let p = ui_tars("Thought: 点击保存按钮 ✓\nAction: type(content='日本語 ✓')").unwrap();
    assert_eq!(p.thought.as_deref(), Some("点击保存按钮 ✓"));
    assert_eq!(image_actions(&p).len(), 1);
    assert!(ui_tars("Action: clïck(start_box='(1,2)')").is_err());
}
