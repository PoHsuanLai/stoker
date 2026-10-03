//! `check_sequence`.

use model_provider::{
    JsonText, Message, Part, Role, SequenceFault, ToolCall, ToolCallId, ToolName, ToolResult,
    ToolStatus, check_sequence,
};
use proptest::prelude::*;

fn text(role: Role) -> Message {
    Message {
        role,
        parts: vec![Part::Text("x".into())],
    }
}

fn call(id: &str) -> Part {
    Part::ToolCall(ToolCall {
        id: ToolCallId(id.into()),
        name: ToolName::new("t").unwrap(),
        input: JsonText::new("{}").unwrap(),
    })
}

fn result(id: &str) -> Part {
    Part::ToolResult(ToolResult {
        id: ToolCallId(id.into()),
        status: ToolStatus::Ok,
        parts: vec![],
    })
}

fn calls(role: Role, ids: &[&str]) -> Message {
    Message {
        role,
        parts: ids.iter().map(|id| call(id)).collect(),
    }
}

fn results(ids: &[&str]) -> Message {
    Message {
        role: Role::Tool,
        parts: ids.iter().map(|id| result(id)).collect(),
    }
}

#[test]
fn orders() {
    let cases: Vec<(&str, Vec<Message>, Result<(), SequenceFault>)> = vec![
        ("empty", vec![], Ok(())),
        (
            "plain chat",
            vec![
                text(Role::System),
                text(Role::User),
                text(Role::Assistant),
                text(Role::User),
            ],
            Ok(()),
        ),
        (
            "one call answered",
            vec![
                text(Role::User),
                calls(Role::Assistant, &["a"]),
                results(&["a"]),
                text(Role::Assistant),
            ],
            Ok(()),
        ),
        (
            "two calls, two results in one message",
            vec![calls(Role::Assistant, &["a", "b"]), results(&["b", "a"])],
            Ok(()),
        ),
        (
            "two calls, results in two messages",
            vec![
                calls(Role::Assistant, &["a", "b"]),
                results(&["a"]),
                results(&["b"]),
                text(Role::User),
            ],
            Ok(()),
        ),
        (
            "two assistants",
            vec![
                text(Role::User),
                text(Role::Assistant),
                text(Role::Assistant),
            ],
            Err(SequenceFault::ConsecutiveAssistant),
        ),
        (
            "assistant call then assistant",
            vec![calls(Role::Assistant, &["a"]), text(Role::Assistant)],
            Err(SequenceFault::ConsecutiveAssistant),
        ),
        (
            "call then user",
            vec![calls(Role::Assistant, &["a"]), text(Role::User)],
            Err(SequenceFault::UnansweredCall),
        ),
        (
            "call at the end",
            vec![text(Role::User), calls(Role::Assistant, &["a"])],
            Err(SequenceFault::UnansweredCall),
        ),
        (
            "one of two answered",
            vec![
                calls(Role::Assistant, &["a", "b"]),
                results(&["a"]),
                text(Role::User),
            ],
            Err(SequenceFault::UnansweredCall),
        ),
        (
            "result with no call",
            vec![text(Role::User), results(&["a"])],
            Err(SequenceFault::OrphanResult),
        ),
        (
            "result for another id",
            vec![calls(Role::Assistant, &["a"]), results(&["b"])],
            Err(SequenceFault::OrphanResult),
        ),
        (
            "a second result for one call",
            vec![
                calls(Role::Assistant, &["a"]),
                results(&["a"]),
                results(&["a"]),
            ],
            Err(SequenceFault::OrphanResult),
        ),
        (
            "result before its call",
            vec![results(&["a"]), calls(Role::Assistant, &["a"])],
            Err(SequenceFault::OrphanResult),
        ),
        (
            "a result in a user message",
            vec![
                calls(Role::Assistant, &["a"]),
                Message {
                    role: Role::User,
                    parts: vec![result("a"), Part::Text("x".into())],
                },
            ],
            Ok(()),
        ),
        (
            "an id reused after it was answered",
            vec![
                calls(Role::Assistant, &["a"]),
                results(&["a"]),
                calls(Role::Assistant, &["a"]),
                results(&["a"]),
            ],
            Ok(()),
        ),
    ];
    for (label, messages, want) in cases {
        assert_eq!(check_sequence(&messages), want, "{label}");
    }
}

proptest! {
    #[test]
    fn well_formed_conversations_pass_and_a_dropped_result_is_caught(
        rounds in proptest::collection::vec(0usize..4, 0..6), drop_at in any::<usize>()
    ) {
        let mut messages = vec![text(Role::System), text(Role::User)];
        let mut next = 0;
        let mut result_slots = Vec::new();
        for n in &rounds {
            if *n == 0 {
                messages.push(text(Role::Assistant));
            } else {
                let ids: Vec<String> = (0..*n).map(|_| { next += 1; format!("c{next}") }).collect();
                let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
                messages.push(calls(Role::Assistant, &refs));
                result_slots.push(messages.len());
                messages.push(results(&refs));
            }
            messages.push(text(Role::User));
        }
        prop_assert_eq!(check_sequence(&messages), Ok(()));
        if !result_slots.is_empty() {
            messages.remove(result_slots[drop_at % result_slots.len()]);
            prop_assert!(check_sequence(&messages).is_err());
        }
    }
}
