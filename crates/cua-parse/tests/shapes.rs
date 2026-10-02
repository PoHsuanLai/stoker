use cua_action::{Coord, CuaAction, GridMax, GridSpace, Point, Target};
use cua_parse::{
    ActionCount, ByteLen, ByteOffset, DropReason, Dropped, InSpace, ParseError, ParseLimits,
    Parsed, VerbText,
};

#[test]
fn parsed_round_trips_with_pinned_json() {
    let parsed = Parsed {
        thought: Some("click save".into()),
        actions: InSpace::Grid(
            GridMax(1000),
            vec![CuaAction::MoveTo {
                at: Target::Point(Point::<GridSpace>::new(Coord(500), Coord(250))),
            }],
        ),
        dropped: vec![Dropped {
            verb: VerbText::new("open_app").unwrap(),
            reason: DropReason::UnsupportedVerb,
        }],
    };
    let json = serde_json::to_string(&parsed).unwrap();
    assert_eq!(
        json,
        r#"{"thought":"click save","actions":{"kind":"grid","v":[1000,[{"kind":"move_to","v":{"at":{"kind":"point","v":{"x":500,"y":250}}}}]]},"dropped":[{"verb":"open_app","reason":"unsupported_verb"}]}"#
    );
    assert_eq!(serde_json::from_str::<Parsed>(&json).unwrap(), parsed);
}

#[test]
fn errors_round_trip() {
    for error in [
        ParseError::NoAction,
        ParseError::Unterminated,
        ParseError::Malformed { at: ByteOffset(12) },
        ParseError::TooLarge,
    ] {
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(serde_json::from_str::<ParseError>(&json).unwrap(), error);
    }
}

#[test]
fn default_limits_are_32_kib_and_8_actions() {
    let limits = ParseLimits::default();
    assert_eq!(limits.max_input, ByteLen(32 * 1024));
    assert_eq!(limits.max_actions, ActionCount(8));
}

#[test]
fn verb_text_refuses_long_and_control_input() {
    assert!(VerbText::new("a".repeat(65)).is_err());
    assert!(VerbText::new("a\nb").is_err());
    assert!(VerbText::new("a".repeat(64)).is_ok());
}
