#![allow(dead_code)]
//! Builders shared by the parser tests.

use std::collections::BTreeSet;

use cua_action::{
    Button, ClickCount, Coord, CuaAction, GridMax, GridSpace, ImageSpace, ModelSpace, Point,
    Target, TextDialect, ToolDialect,
};
use cua_parse::{
    DropReason, InSpace, ParseError, ParseLimits, Parsed, parse_text, parse_tool_calls,
};
use model_provider::{JsonText, ToolCall, ToolCallId, ToolName};

pub fn at(x: u32, y: u32) -> Target<ImageSpace> {
    Target::Point(Point::new(Coord(x), Coord(y)))
}

pub fn gat(x: u32, y: u32) -> Target<GridSpace> {
    Target::Point(Point::new(Coord(x), Coord(y)))
}

pub fn left_click(to: Target<ImageSpace>) -> CuaAction<ImageSpace> {
    CuaAction::Click {
        at: to,
        button: Button::Left,
        count: ClickCount::One,
        mods: BTreeSet::new(),
    }
}

pub fn ui_tars(text: &str) -> Result<Parsed, ParseError> {
    parse_text(
        TextDialect::UiTars15,
        ModelSpace::Image,
        text,
        ParseLimits::default(),
    )
}

pub fn ui_tars_grid(text: &str) -> Result<Parsed, ParseError> {
    let space = ModelSpace::Grid(GridMax(1000));
    parse_text(TextDialect::UiTars15, space, text, ParseLimits::default())
}

pub fn call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: ToolCallId("call_1".into()),
        name: ToolName::new(name).unwrap(),
        input: JsonText::new(arguments).unwrap(),
    }
}

pub fn tools(dialect: ToolDialect, calls: &[ToolCall]) -> Result<Parsed, ParseError> {
    parse_tool_calls(dialect, ModelSpace::Image, calls, ParseLimits::default())
}

pub fn image_actions(parsed: &Parsed) -> &[CuaAction<ImageSpace>] {
    match &parsed.actions {
        InSpace::Image(actions) => actions,
        InSpace::Grid(..) => panic!("expected image space"),
    }
}

pub fn grid_actions(parsed: &Parsed) -> &[CuaAction<GridSpace>] {
    match &parsed.actions {
        InSpace::Grid(_, actions) => actions,
        InSpace::Image(_) => panic!("expected grid space"),
    }
}

pub fn drops(parsed: &Parsed) -> Vec<(&str, DropReason)> {
    parsed
        .dropped
        .iter()
        .map(|d| (d.verb.as_str(), d.reason))
        .collect()
}
