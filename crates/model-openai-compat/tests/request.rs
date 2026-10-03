//! `encode_request`: golden bodies, the messages, and the flavor table, as parsed JSON.

mod support;

use model_openai_compat::{CodecError, Flavor, encode_request, enforceable};
use model_provider::{
    Constraint, Count, Effort, EngineExtras, Knob, LlamaExtras, Milli, OutputShape, Part,
    PromptCache, Reasoning, Role, Seed, SlotId, ToolChoice, ToolName, ToolParallelism, VllmExtras,
};
use serde_json::{Value, json};
use support::*;

const ALL: [Flavor; 4] = [
    Flavor::LlamaServer,
    Flavor::Vllm,
    Flavor::LiteLlm,
    Flavor::OpenRouter,
];

#[test]
fn a_plain_turn_is_the_golden_body() {
    let mut r = base();
    r.messages = vec![msg(Role::System, vec![text("be brief")]), user("hi")];
    assert_eq!(
        body(&r, Flavor::LlamaServer),
        json!({
            "model": "holo",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ],
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": 256,
            "temperature": 0.7,
            "chat_template_kwargs": {"enable_thinking": false},
        })
    );
}

#[test]
fn the_text_of_the_body_is_deterministic_and_streams() {
    let mut r = base();
    r.messages = vec![user("hi")];
    let a = encode_request(&r, Flavor::Vllm).unwrap();
    let b = encode_request(&r, Flavor::Vllm).unwrap();
    assert_eq!(a, b);
    assert!(a.0.contains(r#""stream":true"#));
}

#[test]
fn a_full_request_is_the_golden_body() {
    let mut r = base();
    r.messages = vec![user("open it")];
    r.tools = vec![function("click", r#"{"type":"object","properties":{}}"#)];
    r.tool_choice = ToolChoice::Required;
    r.tool_calls = ToolParallelism::Many;
    r.limits.stop = vec!["</done>".into()];
    r.sampling.top_p = Knob::Set(Milli(950));
    r.sampling.top_k = Knob::Set(Count(20));
    r.sampling.min_p = Knob::Set(Milli(50));
    r.sampling.repeat_penalty = Knob::Set(Milli(1100));
    r.sampling.seed = Knob::Set(Seed(7));
    r.reasoning = Reasoning::On(Effort::High);
    let expected = |penalty: &str, reasoning: Value| {
        let mut want = json!({
            "model": "holo",
            "messages": [{"role": "user", "content": "open it"}],
            "stream": true,
            "stream_options": {"include_usage": true},
            "tools": [{"type": "function", "function": {
                "name": "click", "description": "does click",
                "parameters": {"type": "object", "properties": {}}}}],
            "tool_choice": "required",
            "parallel_tool_calls": true,
            "max_tokens": 256,
            "stop": ["</done>"],
            "temperature": 0.7,
            "top_p": 0.95,
            "top_k": 20,
            "min_p": 0.05,
            "seed": 7,
        });
        want[penalty] = json!(1.1);
        for (key, value) in reasoning.as_object().unwrap() {
            want[key] = value.clone();
        }
        want
    };
    assert_eq!(
        body(&r, Flavor::LlamaServer),
        expected(
            "repeat_penalty",
            json!({"chat_template_kwargs": {"enable_thinking": true}})
        )
    );
    assert_eq!(
        body(&r, Flavor::Vllm),
        expected(
            "repetition_penalty",
            json!({"chat_template_kwargs": {"enable_thinking": true}, "reasoning_effort": "high"})
        )
    );
    assert_eq!(
        body(&r, Flavor::LiteLlm),
        expected("repetition_penalty", json!({"reasoning_effort": "high"}))
    );
    assert_eq!(body(&r, Flavor::OpenRouter), {
        let mut want = expected(
            "repetition_penalty",
            json!({"reasoning": {"effort": "high"}}),
        );
        want.as_object_mut().unwrap().remove("stream_options");
        want
    });
}

#[test]
fn milli_values_print_as_the_decimal_a_person_wrote() {
    for (milli, text) in [
        (0, "0.0"),
        (50, "0.05"),
        (700, "0.7"),
        (1100, "1.1"),
        (1000, "1.0"),
    ] {
        let mut r = base();
        r.sampling.temperature = Milli(milli);
        let json = encode_request(&r, Flavor::Vllm).unwrap().0;
        assert!(
            json.contains(&format!(r#""temperature":{text}"#)),
            "{milli}: {json}"
        );
    }
}

#[test]
fn usage_is_asked_for_except_where_the_server_rejects_it() {
    for flavor in ALL {
        let has = body(&base(), flavor).get("stream_options").is_some();
        assert_eq!(has, flavor != Flavor::OpenRouter, "{flavor:?}");
    }
}

#[test]
fn the_tool_choice_spellings() {
    let mut r = base();
    r.tools = vec![function("click", "{}")];
    for (choice, want) in [
        (ToolChoice::Auto, json!("auto")),
        (ToolChoice::Never, json!("none")),
        (ToolChoice::Required, json!("required")),
    ] {
        r.tool_choice = choice;
        assert_eq!(body(&r, Flavor::LlamaServer)["tool_choice"], want);
    }
    r.tool_choice = ToolChoice::Named(ToolName::new("click").unwrap());
    assert_eq!(
        body(&r, Flavor::Vllm)["tool_choice"],
        json!({"type": "function", "function": {"name": "click"}})
    );
}

#[test]
fn named_tool_refused_on_llama_server() {
    let mut r = base();
    r.tools = vec![function("click", "{}")];
    r.tool_choice = ToolChoice::Named(ToolName::new("click").unwrap());
    assert_eq!(
        encode_request(&r, Flavor::LlamaServer),
        Err(CodecError::UnsupportedShape)
    );
    // Silently treated as `auto` by the server, so refused even with no tools to name.
    r.tools.clear();
    assert_eq!(
        encode_request(&r, Flavor::LlamaServer),
        Err(CodecError::UnsupportedShape)
    );
    for flavor in [Flavor::Vllm, Flavor::LiteLlm, Flavor::OpenRouter] {
        assert!(encode_request(&r, flavor).is_ok(), "{flavor:?}");
    }
}

#[test]
fn no_tools_means_no_tool_fields() {
    let mut r = base();
    r.tool_choice = ToolChoice::Required;
    let b = body(&r, Flavor::Vllm);
    for key in ["tools", "tool_choice", "parallel_tool_calls"] {
        assert!(b.get(key).is_none(), "{key}");
    }
}

#[test]
fn a_native_tool_is_refused() {
    use model_provider::{NativeTool, ToolSpec};
    let native: NativeTool =
        serde_json::from_value(json!({"dialect": "open_ai_computer", "config": "{}"})).unwrap();
    let mut r = base();
    r.tools = vec![ToolSpec::Native(native)];
    for flavor in ALL {
        assert_eq!(
            encode_request(&r, flavor),
            Err(CodecError::NativeToolUnsupported)
        );
    }
    r.tools = vec![function("click", "{}")];
    assert!(encode_request(&r, Flavor::Vllm).is_ok());
}

#[test]
fn a_user_message_is_a_string_or_a_list_of_parts() {
    let mut r = base();
    r.messages = vec![
        user("plain"),
        msg(
            Role::User,
            vec![text("look"), Part::Image(image("png", b"abc"))],
        ),
        msg(Role::User, vec![text("a"), text("b")]),
    ];
    let b = body(&r, Flavor::Vllm);
    assert_eq!(b["messages"][0]["content"], json!("plain"));
    assert_eq!(
        b["messages"][1]["content"],
        json!([
            {"type": "text", "text": "look"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,YWJj"}},
        ])
    );
    assert_eq!(
        b["messages"][2]["content"],
        json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}])
    );
}

#[test]
fn an_original_detail_image_asks_for_high_detail() {
    let mut original = image("jpeg", b"abc");
    original.detail = model_provider::ImageDetail::Original;
    let mut r = base();
    r.messages = vec![msg(Role::User, vec![Part::Image(original)])];
    assert_eq!(
        body(&r, Flavor::Vllm)["messages"][0]["content"][0]["image_url"],
        json!({"url": "data:image/jpeg;base64,YWJj", "detail": "high"})
    );
}

#[test]
fn an_assistant_turn_carries_its_text_and_calls_and_drops_its_thoughts() {
    let mut r = base();
    r.messages = vec![
        user("go"),
        msg(
            Role::Assistant,
            vec![
                Part::Thought {
                    text: "hmm".into(),
                    seal: model_provider::ThoughtSeal::None,
                },
                text("ok"),
                Part::ToolCall(call("c1", "click", r#"{"x":1}"#)),
                Part::ToolCall(call("c2", "click", "{}")),
            ],
        ),
    ];
    assert_eq!(
        body(&r, Flavor::Vllm)["messages"][1],
        json!({
            "role": "assistant",
            "content": "ok",
            "tool_calls": [
                {"id": "c1", "type": "function", "function": {"name": "click", "arguments": "{\"x\":1}"}},
                {"id": "c2", "type": "function", "function": {"name": "click", "arguments": "{}"}},
            ],
        })
    );
    let mut calls_only = base();
    calls_only.messages = vec![msg(
        Role::Assistant,
        vec![Part::ToolCall(call("c1", "click", "{}"))],
    )];
    assert_eq!(
        body(&calls_only, Flavor::Vllm)["messages"][0]["content"],
        json!("")
    );
}

#[test]
fn tool_results_are_tool_messages_split_out_of_their_message() {
    let mut r = base();
    r.messages = vec![
        msg(
            Role::Assistant,
            vec![
                Part::ToolCall(call("c1", "a", "{}")),
                Part::ToolCall(call("c2", "b", "{}")),
            ],
        ),
        msg(
            Role::User,
            vec![
                result("c1", vec![text("one"), text("two")]),
                result("c2", vec![text("three")]),
                text("and now?"),
            ],
        ),
        msg(Role::Tool, vec![result("c3", vec![text("late")])]),
    ];
    let b = body(&r, Flavor::Vllm);
    let m = b["messages"].as_array().unwrap();
    assert_eq!(m.len(), 5);
    assert_eq!(
        m[1],
        json!({"role": "tool", "tool_call_id": "c1", "content": "one\ntwo"})
    );
    assert_eq!(
        m[2],
        json!({"role": "tool", "tool_call_id": "c2", "content": "three"})
    );
    assert_eq!(m[3], json!({"role": "user", "content": "and now?"}));
    assert_eq!(
        m[4],
        json!({"role": "tool", "tool_call_id": "c3", "content": "late"})
    );
}

#[test]
fn an_image_in_a_tool_result_goes_where_the_flavor_honours_it() {
    let mut r = base();
    r.messages = vec![
        msg(
            Role::Assistant,
            vec![
                Part::ToolCall(call("c1", "shot", "{}")),
                Part::ToolCall(call("c2", "shot", "{}")),
            ],
        ),
        msg(
            Role::Tool,
            vec![
                result(
                    "c1",
                    vec![text("screen"), Part::Image(image("png", b"abc"))],
                ),
                result("c2", vec![Part::Image(image("png", b"abc"))]),
            ],
        ),
        user("next"),
    ];
    let picture = json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,YWJj"}});
    let llama = body(&r, Flavor::LlamaServer);
    let m = llama["messages"].as_array().unwrap();
    assert_eq!(
        m[1],
        json!({"role": "tool", "tool_call_id": "c1", "content": [
            {"type": "text", "text": "screen"}, picture.clone()]})
    );
    assert_eq!(m.len(), 4, "no extra user message");

    let vllm = body(&r, Flavor::Vllm);
    let m = vllm["messages"].as_array().unwrap();
    assert_eq!(m.len(), 5);
    assert_eq!(
        m[1],
        json!({"role": "tool", "tool_call_id": "c1", "content": "screen"})
    );
    assert_eq!(
        m[2],
        json!({"role": "tool", "tool_call_id": "c2", "content": ""})
    );
    assert_eq!(
        m[3],
        json!({"role": "user", "content": [picture.clone(), picture]}),
        "the images follow once every tool message is over"
    );
    assert_eq!(m[4], json!({"role": "user", "content": "next"}));
}

#[test]
fn trailing_tool_images_are_not_lost() {
    let mut r = base();
    r.messages = vec![
        msg(
            Role::Assistant,
            vec![Part::ToolCall(call("c1", "shot", "{}"))],
        ),
        msg(
            Role::Tool,
            vec![result("c1", vec![Part::Image(image("png", b"abc"))])],
        ),
    ];
    let b = body(&r, Flavor::Vllm);
    assert_eq!(b["messages"].as_array().unwrap().len(), 3);
    assert_eq!(b["messages"][2]["role"], json!("user"));
}

#[test]
fn a_system_message_joins_its_text_and_refuses_an_image() {
    let mut r = base();
    r.messages = vec![msg(Role::System, vec![text("a"), text("b")])];
    assert_eq!(
        body(&r, Flavor::Vllm)["messages"][0]["content"],
        json!("a\n\nb")
    );
    r.messages = vec![msg(Role::System, vec![Part::Image(image("png", b"x"))])];
    assert_eq!(
        encode_request(&r, Flavor::Vllm),
        Err(CodecError::UnsupportedShape)
    );
    r.messages = vec![msg(Role::Assistant, vec![Part::Image(image("png", b"x"))])];
    assert_eq!(
        encode_request(&r, Flavor::Vllm),
        Err(CodecError::UnsupportedShape)
    );
}

#[test]
fn a_message_with_no_parts_is_left_out() {
    let mut r = base();
    r.messages = vec![msg(Role::User, vec![]), user("hi")];
    assert_eq!(
        body(&r, Flavor::Vllm)["messages"].as_array().unwrap().len(),
        1
    );
}

fn schema() -> OutputShape {
    OutputShape::JsonSchema(model_provider::SchemaText(
        model_provider::JsonText::new(r#"{"type":"object"}"#).unwrap(),
    ))
}

#[test]
fn a_json_schema_is_a_strict_response_format_everywhere() {
    let mut r = base();
    r.output = schema();
    for flavor in ALL {
        assert_eq!(
            body(&r, flavor)["response_format"],
            json!({"type": "json_schema", "json_schema": {
                "name": "reply", "strict": true, "schema": {"type": "object"}}}),
            "{flavor:?}"
        );
    }
}

#[test]
fn the_other_constraints_are_spelled_per_engine() {
    let mut r = base();
    r.output = OutputShape::Gbnf("root ::= \"a\"".into());
    assert_eq!(
        body(&r, Flavor::LlamaServer)["grammar"],
        json!("root ::= \"a\"")
    );
    r.output = OutputShape::Choice(vec!["allow".into(), "say \"no\"".into(), "a\\b".into()]);
    assert_eq!(
        body(&r, Flavor::LlamaServer)["grammar"],
        json!(r#"root ::= "allow" | "say \"no\"" | "a\\b""#)
    );
    assert_eq!(
        body(&r, Flavor::Vllm)["structured_outputs"],
        json!({"choice": ["allow", "say \"no\"", "a\\b"]})
    );
    r.output = OutputShape::Regex("[a-z]+".into());
    assert_eq!(
        body(&r, Flavor::Vllm)["structured_outputs"],
        json!({"regex": "[a-z]+"})
    );
    r.output = OutputShape::Lark("start: \"a\"".into());
    assert_eq!(
        body(&r, Flavor::Vllm)["structured_outputs"],
        json!({"grammar": "start: \"a\""})
    );
}

#[test]
fn what_a_flavor_cannot_enforce_is_refused_not_dropped() {
    let shapes = [
        (Constraint::JsonSchema, schema()),
        (Constraint::Regex, OutputShape::Regex("a".into())),
        (Constraint::Lark, OutputShape::Lark("start: \"a\"".into())),
        (Constraint::Gbnf, OutputShape::Gbnf("root ::= \"a\"".into())),
        (Constraint::Choice, OutputShape::Choice(vec!["a".into()])),
    ];
    for flavor in ALL {
        for (kind, output) in &shapes {
            let mut r = base();
            r.output = output.clone();
            let accepted = encode_request(&r, flavor).is_ok();
            assert_eq!(
                accepted,
                enforceable(flavor).contains(kind),
                "{flavor:?} {kind:?}"
            );
        }
    }
}

#[test]
fn a_constraint_waits_for_a_tool_result_where_the_flavor_says_after_result() {
    let mut r = base();
    r.tools = vec![function("click", "{}")];
    r.output = schema();
    r.messages = vec![user("go")];
    // llama-server and LiteLLM: `AfterResult`; vLLM: `Together`.
    assert!(
        body(&r, Flavor::LlamaServer)
            .get("response_format")
            .is_none()
    );
    assert!(body(&r, Flavor::Vllm).get("response_format").is_some());
    r.messages = vec![
        user("go"),
        msg(
            Role::Assistant,
            vec![Part::ToolCall(call("c1", "click", "{}"))],
        ),
        msg(Role::Tool, vec![result("c1", vec![text("done")])]),
    ];
    for flavor in ALL {
        assert!(
            body(&r, flavor).get("response_format").is_some(),
            "{flavor:?}"
        );
    }
}

#[test]
fn engine_extras_reach_only_their_own_flavor() {
    let llama = EngineExtras::LlamaServer(LlamaExtras {
        cache_prompt: PromptCache::Fresh,
        slot: Knob::Set(SlotId(2)),
    });
    let vllm = EngineExtras::Vllm(VllmExtras {
        priority: Knob::Set(Count(5)),
    });
    let mut r = base();
    r.engine = llama;
    let b = body(&r, Flavor::LlamaServer);
    assert_eq!(
        (b["cache_prompt"].clone(), b["id_slot"].clone()),
        (json!(false), json!(2))
    );
    assert!(body(&r, Flavor::Vllm).get("cache_prompt").is_none());
    r.engine = vllm;
    assert_eq!(body(&r, Flavor::Vllm)["priority"], json!(5));
    assert!(body(&r, Flavor::LlamaServer).get("priority").is_none());
    r.engine = EngineExtras::LlamaServer(LlamaExtras {
        cache_prompt: PromptCache::Reuse,
        slot: Knob::Off,
    });
    let b = body(&r, Flavor::LlamaServer);
    assert_eq!(b["cache_prompt"], json!(true));
    assert!(b.get("id_slot").is_none());
}

#[test]
fn reasoning_off_switches_thinking_off_where_there_is_a_switch() {
    for flavor in ALL {
        let b = body(&base(), flavor);
        let off = flavor == Flavor::LlamaServer || flavor == Flavor::Vllm;
        assert_eq!(b.get("chat_template_kwargs").is_some(), off, "{flavor:?}");
        assert!(b.get("reasoning_effort").is_none());
        assert!(b.get("reasoning").is_none());
    }
}

#[test]
fn a_request_is_valid_json_for_any_text() {
    use proptest::prelude::*;
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&(".{0,60}", ".{0,60}"), |(system, user_text)| {
            let mut r = base();
            r.messages = vec![msg(Role::System, vec![text(&system)]), user(&user_text)];
            for flavor in ALL {
                let json = encode_request(&r, flavor).unwrap().0;
                let b: Value = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(
                    b["messages"][1]["content"].as_str(),
                    Some(user_text.as_str())
                );
                prop_assert_eq!(b["messages"][0]["content"].as_str(), Some(system.as_str()));
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn reasoning_engine_default_sends_no_switch_and_off_still_does() {
    for flavor in ALL {
        let mut r = base();
        r.messages = vec![user("hi")];
        r.reasoning = Reasoning::EngineDefault;
        let default = body(&r, flavor);
        for key in ["chat_template_kwargs", "reasoning_effort", "reasoning"] {
            assert!(default.get(key).is_none(), "{flavor:?} sent {key}");
        }
        r.reasoning = Reasoning::Off;
        let off = body(&r, flavor);
        let switched = matches!(flavor, Flavor::LlamaServer | Flavor::Vllm);
        assert_eq!(
            off.get("chat_template_kwargs") == Some(&json!({"enable_thinking": false})),
            switched,
            "{flavor:?}"
        );
    }
}
