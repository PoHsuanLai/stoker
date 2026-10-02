//! `parse_tool_calls` is total: the first byte picks the dialect and space, the rest is split
//! on a blank line into calls, each `name`, a NUL, then arguments. Names and arguments the
//! boundary types refuse are skipped, as an engine's output would be refused before it got here.
#![no_main]

use cua_action::{GridMax, ModelSpace, ToolDialect};
use cua_parse::{InSpace, ParseLimits, parse_tool_calls};
use libfuzzer_sys::fuzz_target;
use model_provider::{JsonText, ToolCall, ToolCallId, ToolName};

fuzz_target!(|data: &[u8]| {
    let Some((head, rest)) = data.split_first() else {
        return;
    };
    let dialect = if head & 1 == 0 {
        ToolDialect::QwenComputerUse
    } else {
        ToolDialect::Holo31
    };
    let space = if head & 2 == 0 {
        ModelSpace::Image
    } else {
        ModelSpace::Grid(GridMax(1000))
    };
    let text = String::from_utf8_lossy(rest);
    let calls: Vec<ToolCall> = text
        .split("\n\n")
        .filter_map(|chunk| {
            let (name, args) = chunk.split_once('\0')?;
            Some(ToolCall {
                id: ToolCallId("fuzz".into()),
                name: ToolName::new(name).ok()?,
                input: JsonText::new(args).ok()?,
            })
        })
        .collect();
    let limits = ParseLimits::default();
    if let Ok(parsed) = parse_tool_calls(dialect, space, &calls, limits) {
        let kept = match &parsed.actions {
            InSpace::Image(actions) => actions.len(),
            InSpace::Grid(_, actions) => actions.len(),
        };
        assert!(kept <= usize::from(limits.max_actions.0));
    }
});
