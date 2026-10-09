//! A session over a vendor wire: real tool-call history, the vendor's results first in the next
//! user message, and a text reply that ends the run.

use crate::support;

use cua_action::{CuaAction, CuaDialect, FinishOutcome, ModelSpace, WireDialect};
use cua_session::{StepOutcome, TurnTranscript};
use cua_vendors::StepResult;
use model_provider::{Part, Role, StopReason, ToolCallId, ToolSpec, check_sequence};
use support::*;

const ANTHROPIC: CuaDialect = CuaDialect::Wire(WireDialect::AnthropicToolset20260801);

fn tool_use(name: &str, input: &str, id: &str) -> TurnTranscript {
    let mut reply = calls(&[]);
    let mut c = call(name, input);
    c.id = ToolCallId(id.into());
    reply.calls = vec![c];
    reply
}

#[test]
fn the_history_is_real_messages_and_the_results_lead_the_next_turn() {
    let m = map(ModelSpace::Image);
    let s = session(ANTHROPIC, ModelSpace::Image, 1, 1);
    let first = s.request(&obs(0), &m, frame("f0"));
    assert!(matches!(&first.tools[..], [ToolSpec::Native(_)]));
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].role, Role::User);
    assert_eq!(
        first.messages[0].parts.last(),
        Some(&Part::Image(frame("f0")))
    );

    let reply = tool_use("left_click", r#"{"coordinate":[100,50]}"#, "toolu_1");
    let mut s = s;
    let outcome = s.absorb_for(&first, reply, &m);
    let StepOutcome::Actions { actions, .. } = outcome else {
        panic!("actions")
    };
    assert!(matches!(&actions[..], [CuaAction::Click { .. }]));

    let mut o = obs(1);
    o.prev = vec![StepResult::Done(ToolCallId("toolu_1".into()))];
    let second = s.request(&o, &m, frame("f1"));
    let roles: Vec<Role> = second.messages.iter().map(|m| m.role).collect();
    assert_eq!(roles, [Role::User, Role::Assistant, Role::User]);
    assert!(matches!(&second.messages[1].parts[..], [Part::ToolCall(c)] if c.id.0 == "toolu_1"));
    // The results come first, and carry the new frame; the goal is not said again.
    let user = &second.messages[2].parts;
    assert!(matches!(&user[0], Part::ToolResult(r) if r.id.0 == "toolu_1"));
    assert!(
        user.iter()
            .all(|p| !matches!(p, Part::Text(t) if t.contains("Goal:")))
    );
    assert_eq!(check_sequence(&second.messages), Ok(()));
    // The first frame is still there (history of one frame), then it ages out.
    assert!(matches!(&second.messages[0].parts.last(), Some(Part::Image(i)) if *i == frame("f0")));
    s.absorb_for(&second, tool_use("type", r#"{"text":"x"}"#, "toolu_2"), &m);
    let mut o = obs(2);
    o.prev = vec![StepResult::Done(ToolCallId("toolu_2".into()))];
    let third = s.request(&o, &m, frame("f2"));
    assert_eq!(third.messages.len(), 5);
    assert!(
        third.messages[0]
            .parts
            .iter()
            .all(|p| !matches!(p, Part::Image(_))),
        "the oldest frame is gone"
    );
    assert_eq!(check_sequence(&third.messages), Ok(()));
}

#[test]
fn a_reply_of_text_alone_finishes_the_run() {
    let m = map(ModelSpace::Image);
    let s = session(ANTHROPIC, ModelSpace::Image, 1, 1);
    let sent = s.request(&obs(0), &m, frame("f"));
    let mut reply = calls(&[]);
    reply.text = "The file is saved.".into();
    reply.end.stop = StopReason::EndTurn;
    let outcome = s.clone().absorb_for(&sent, reply, &m);
    let StepOutcome::Actions { actions, .. } = outcome else {
        panic!("actions")
    };
    let [
        CuaAction::Finish {
            outcome, summary, ..
        },
    ] = &actions[..]
    else {
        panic!("a finish")
    };
    assert_eq!(
        (*outcome, summary.as_str()),
        (FinishOutcome::Done, "The file is saved.")
    );
    // Nothing at all is not a finish.
    let outcome = s.clone().absorb_for(&sent, calls(&[]), &m);
    assert!(matches!(outcome, StepOutcome::Repair(_)));
}

#[test]
fn gemini_needs_a_grid_map_and_decodes_into_it() {
    let dialect = CuaDialect::Wire(WireDialect::GeminiComputerUse);
    let m = map(grid());
    let s = session(dialect, grid(), 0, 0);
    let sent = s.request(&obs(0), &m, frame("f"));
    let mut reply = calls(&[]);
    let mut c = call("click", r#"{"x":500,"y":250}"#);
    c.id = ToolCallId("g1".into());
    reply.calls = vec![c];
    let outcome = s.clone().absorb_for(&sent, reply, &m);
    let StepOutcome::Actions {
        actions, dropped, ..
    } = outcome
    else {
        panic!("actions")
    };
    assert!(dropped.is_empty());
    assert!(matches!(&actions[..], [CuaAction::Click { .. }]));
}
