use genai_names::{Finish, Operation, attr, metric};

#[test]
fn operation_names_are_the_convention_values() {
    const CASES: &[(Operation, &str)] = &[
        (Operation::Chat, "chat"),
        (Operation::Embeddings, "embeddings"),
        (Operation::ExecuteTool, "execute_tool"),
        (Operation::InvokeAgent, "invoke_agent"),
        (Operation::TextCompletion, "text_completion"),
        (Operation::GenerateContent, "generate_content"),
    ];
    assert_eq!(CASES.len(), Operation::ALL.len());
    for (op, name) in CASES {
        assert_eq!(op.as_str(), *name);
    }
    // Streaming is not an operation (rig invented `chat_streaming`; the convention has none).
    assert!(
        Operation::ALL
            .iter()
            .all(|o| !o.as_str().contains("streaming"))
    );
}

#[test]
fn finish_reasons_are_the_convention_values() {
    const CASES: &[(Finish, &str)] = &[
        (Finish::Stop, "stop"),
        (Finish::Length, "length"),
        (Finish::ToolCalls, "tool_calls"),
        (Finish::ContentFilter, "content_filter"),
        (Finish::Error, "error"),
    ];
    assert_eq!(CASES.len(), Finish::ALL.len());
    for (finish, name) in CASES {
        assert_eq!(finish.as_str(), *name);
    }
}

#[test]
fn attribute_keys_are_unique_and_namespaced() {
    let mut seen = std::collections::BTreeSet::new();
    for key in attr::ALL {
        assert!(seen.insert(*key), "{key} is listed twice");
        assert!(
            key.starts_with("gen_ai.") || matches!(*key, "error.type" | "server.address"),
            "{key} is outside the conventions' namespaces"
        );
    }
    assert_eq!(attr::ALL.len(), 19);
}

#[test]
fn metric_names_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for name in metric::ALL {
        assert!(seen.insert(*name));
        assert!(name.starts_with("gen_ai."));
    }
}

#[test]
fn no_content_attribute_exists() {
    // Prompts, completions and tool arguments are personal: they have no key here.
    for key in attr::ALL {
        for forbidden in [
            "input.messages",
            "output.messages",
            "system_instructions",
            "call.arguments",
            "call.result",
        ] {
            assert!(!key.contains(forbidden), "{key} would carry content");
        }
    }
}
