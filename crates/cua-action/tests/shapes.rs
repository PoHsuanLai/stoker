//! Shape tests: the serde form of every action variant, the limits, the class table, the map.

use std::collections::BTreeSet;

use cua_action::{
    ActionClass, Button, Choice, Chord, ChordError, ClickCount, Coord, CuaAction, CuaDialect,
    Extracted, ExtractedText, FieldName, FinishOutcome, GridMax, GridSpace, ImageSpace, Length,
    ModelSpace, Modifier, NodeId, Notches, Point, Rect, Repeat, ScrollBy, ScrollDir, Size, Summary,
    Target, TextDialect, TextError, ToolDialect, TypedText, WaitMs, WindowSpace, WireDialect,
};
use keyboard_types::Key;

fn p(x: u32, y: u32) -> Point<WindowSpace> {
    Point::new(Coord(x), Coord(y))
}

fn at(x: u32, y: u32) -> Target<WindowSpace> {
    Target::Point(p(x, y))
}

fn every_action() -> Vec<CuaAction<WindowSpace>> {
    let chord = Chord::new(BTreeSet::from([Modifier::Ctrl]), Key::Character("a".into())).unwrap();
    vec![
        CuaAction::Click {
            at: at(10, 20),
            button: Button::Left,
            count: ClickCount::Two,
            mods: BTreeSet::from([Modifier::Shift]),
        },
        CuaAction::MoveTo {
            at: Target::Node(NodeId(7)),
        },
        CuaAction::Drag {
            from: at(1, 2),
            to: at(3, 4),
            button: Button::Right,
        },
        CuaAction::Type {
            text: TypedText::new("hello\nworld").unwrap(),
        },
        CuaAction::Key {
            chord,
            repeat: Repeat::new(3).unwrap(),
        },
        CuaAction::Scroll {
            at: at(5, 6),
            dir: ScrollDir::Down,
            by: ScrollBy::Distance(Length::new(Coord(120))),
        },
        CuaAction::Scroll {
            at: at(5, 6),
            dir: ScrollDir::Up,
            by: ScrollBy::Notches(Notches(3)),
        },
        CuaAction::Wait {
            for_ms: WaitMs::new(500).unwrap(),
        },
        CuaAction::Zoom {
            region: Rect {
                origin: p(0, 0),
                size: Size::new(Coord(64), Coord(48)),
            },
        },
        CuaAction::Observe,
        CuaAction::Finish {
            outcome: FinishOutcome::Done,
            summary: Summary::new("sent").unwrap(),
            extracted: vec![Extracted {
                name: FieldName::new("total").unwrap(),
                value: ExtractedText::new("12.50").unwrap(),
            }],
        },
        CuaAction::Ask {
            question: Summary::new("Which one?").unwrap(),
            choices: vec![
                Choice::new("first").unwrap(),
                Choice::new("second").unwrap(),
            ],
        },
    ]
}

#[test]
fn actions_round_trip() {
    for action in every_action() {
        let json = serde_json::to_string(&action).unwrap();
        let back: CuaAction<WindowSpace> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action, "{json}");
    }
}

#[test]
fn click_json_is_pinned() {
    let json = serde_json::to_string(&every_action()[0]).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"click","v":{"at":{"kind":"point","v":{"x":10,"y":20}},"button":"left","count":"two","mods":["shift"]}}"#
    );
}

#[test]
fn node_observe_and_scroll_json_are_pinned() {
    let all = every_action();
    assert_eq!(
        serde_json::to_string(&all[1]).unwrap(),
        r#"{"kind":"move_to","v":{"at":{"kind":"node","v":7}}}"#
    );
    assert_eq!(
        serde_json::to_string(&all[9]).unwrap(),
        r#"{"kind":"observe"}"#
    );
    assert_eq!(
        serde_json::to_string(&all[5]).unwrap(),
        r#"{"kind":"scroll","v":{"at":{"kind":"point","v":{"x":5,"y":6}},"dir":"down","by":{"kind":"distance","v":120}}}"#
    );
}

#[test]
fn key_json_is_pinned() {
    let json = serde_json::to_string(&every_action()[4]).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"key","v":{"chord":{"mods":["ctrl"],"key":{"Character":"a"}},"repeat":3}}"#
    );
}

#[test]
fn dialects_round_trip_and_slugs_are_pinned() {
    const CASES: &[(CuaDialect, &str)] = &[
        (
            CuaDialect::Wire(WireDialect::AnthropicToolset20260801),
            r#"{"kind":"wire","v":"anthropic_toolset20260801"}"#,
        ),
        (
            CuaDialect::Wire(WireDialect::AnthropicComputer20251124),
            r#"{"kind":"wire","v":"anthropic_computer20251124"}"#,
        ),
        (
            CuaDialect::Wire(WireDialect::OpenAiComputer),
            r#"{"kind":"wire","v":"open_ai_computer"}"#,
        ),
        (
            CuaDialect::Wire(WireDialect::GeminiComputerUse),
            r#"{"kind":"wire","v":"gemini_computer_use"}"#,
        ),
        (
            CuaDialect::Text(TextDialect::UiTars15),
            r#"{"kind":"text","v":"ui_tars15"}"#,
        ),
        (
            CuaDialect::Tool(ToolDialect::QwenComputerUse),
            r#"{"kind":"tool","v":"qwen_computer_use"}"#,
        ),
        (
            CuaDialect::Tool(ToolDialect::Holo31),
            r#"{"kind":"tool","v":"holo31"}"#,
        ),
    ];
    for (dialect, json) in CASES {
        assert_eq!(
            &serde_json::to_string(dialect).unwrap(),
            json,
            "{dialect:?}"
        );
        assert_eq!(&serde_json::from_str::<CuaDialect>(json).unwrap(), dialect);
    }
    let space = ModelSpace::Grid(GridMax(1000));
    assert_eq!(
        serde_json::to_string(&space).unwrap(),
        r#"{"kind":"grid","v":1000}"#
    );
}

#[test]
fn text_limits_refuse() {
    let long = "a".repeat(2049);
    assert_eq!(TypedText::new(""), Err(TextError::Empty));
    assert_eq!(
        TypedText::new(long),
        Err(TextError::TooLong {
            max: 2048,
            got: 2049
        })
    );
    assert_eq!(
        TypedText::new("a\u{7}b"),
        Err(TextError::ControlChar { at: 1 })
    );
    assert!(TypedText::new("tab\there\nnewline").is_ok());
    assert!(TypedText::new("a".repeat(2048)).is_ok());
    assert_eq!(Choice::new("a\nb"), Err(TextError::ControlChar { at: 1 }));
    assert_eq!(FieldName::new(""), Err(TextError::Empty));
    assert!(serde_json::from_str::<TypedText>("\"a\\u0007\"").is_err());
}

#[test]
fn number_limits_refuse() {
    assert!(Repeat::new(0).is_err());
    assert!(Repeat::new(1).is_ok());
    assert!(Repeat::new(20).is_ok());
    assert!(Repeat::new(21).is_err());
    assert!(WaitMs::new(60_000).is_ok());
    assert!(WaitMs::new(60_001).is_err());
    assert!(serde_json::from_str::<Repeat>("0").is_err());
    assert!(serde_json::from_str::<WaitMs>("60001").is_err());
}

#[test]
fn chords_refuse_unidentified_and_dead_keys() {
    let none = BTreeSet::new();
    assert_eq!(
        Chord::new(none.clone(), Key::Unidentified),
        Err(ChordError::Unidentified)
    );
    assert_eq!(Chord::new(none, Key::Dead), Err(ChordError::Dead));
}

#[test]
fn debug_redacts_typed_text() {
    let text = TypedText::new("my password").unwrap();
    assert_eq!(format!("{text:?}"), "TypedText(<11 chars>)");
}

#[test]
fn class_of_every_variant() {
    const EXPECTED: &[ActionClass] = &[
        ActionClass::Pointer,  // click
        ActionClass::Pointer,  // move_to
        ActionClass::Pointer,  // drag
        ActionClass::Keyboard, // type
        ActionClass::Keyboard, // key
        ActionClass::Pointer,  // scroll (distance)
        ActionClass::Pointer,  // scroll (notches)
        ActionClass::Observe,  // wait
        ActionClass::Observe,  // zoom
        ActionClass::Observe,  // observe
        ActionClass::Conclude, // finish
        ActionClass::Conclude, // ask
    ];
    let actions = every_action();
    assert_eq!(actions.len(), EXPECTED.len());
    for (action, want) in actions.iter().zip(EXPECTED) {
        assert_eq!(action.class(), *want, "{action:?}");
    }
}

#[test]
fn map_points_halves_points_and_lengths_and_passes_nodes() {
    let half_point = |q: Point<WindowSpace>| -> Result<Point<ImageSpace>, ()> {
        Ok(Point::new(Coord(q.x.0 / 2), Coord(q.y.0 / 2)))
    };
    let half_len = |l: Length<WindowSpace>| -> Result<Length<ImageSpace>, ()> {
        Ok(Length::new(Coord(l.coord().0 / 2)))
    };
    let mapped: Vec<CuaAction<ImageSpace>> = every_action()
        .into_iter()
        .map(|a| a.map_points(half_point, half_len).unwrap())
        .collect();
    assert_eq!(
        mapped[0],
        CuaAction::Click {
            at: Target::Point(Point::new(Coord(5), Coord(10))),
            button: Button::Left,
            count: ClickCount::Two,
            mods: BTreeSet::from([Modifier::Shift]),
        }
    );
    assert_eq!(
        mapped[1],
        CuaAction::MoveTo {
            at: Target::Node(NodeId(7))
        }
    );
    assert_eq!(
        mapped[5],
        CuaAction::Scroll {
            at: Target::Point(Point::new(Coord(2), Coord(3))),
            dir: ScrollDir::Down,
            by: ScrollBy::Distance(Length::new(Coord(60))),
        }
    );
    assert_eq!(
        mapped[8],
        CuaAction::Zoom {
            region: Rect {
                origin: Point::new(Coord(0), Coord(0)),
                size: Size::new(Coord(32), Coord(24))
            }
        }
    );
}

#[test]
fn map_points_stops_at_the_first_error() {
    let refuse =
        |_: Point<WindowSpace>| -> Result<Point<GridSpace>, &'static str> { Err("out of frame") };
    let keep = |l: Length<WindowSpace>| -> Result<Length<GridSpace>, &'static str> {
        Ok(Length::new(l.coord()))
    };
    let click = every_action().remove(0);
    assert_eq!(click.map_points(refuse, keep), Err("out of frame"));
}
