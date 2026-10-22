//! Calls read from the reply text for a model whose server does not parse them: the Qwen3 XML
//! and the Hermes JSON formats, a call written inside the reasoning, the bounds, and the forms
//! that are not read.

use cua_action::CallFormat;
use cua_parse::{ActionCount, ByteLen, ParamKinds, ParseError, ParseLimits, read_calls};
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"properties":{"action":{"type":"string"},"text":{"type":"string"},"coordinate":{"type":"array"},"pixels":{"type":"integer"}}}"#;

fn xml(action: &str, extra: &str) -> String {
    format!(
        "<tool_call>\n<function=computer_use>\n<parameter=action>\n{action}\n</parameter>\n{extra}</function>\n</tool_call>"
    )
}

fn parameter(name: &str, value: &str) -> String {
    format!("<parameter={name}>\n{value}\n</parameter>\n")
}

fn read(
    format: CallFormat,
    answer: &str,
    thought: &str,
    kinds: &ParamKinds,
) -> Result<Vec<(String, Value)>, ParseError> {
    read_calls(format, answer, thought, kinds, ParseLimits::default()).map(|calls| {
        calls
            .iter()
            .map(|c| {
                let input = serde_json::from_str(c.input.as_str()).unwrap();
                (c.name.as_str().to_owned(), input)
            })
            .collect()
    })
}

fn computer_use(input: Value) -> (String, Value) {
    ("computer_use".to_owned(), input)
}

#[test]
fn qwen3_xml_calls_are_read_with_the_types_the_schema_gives() {
    let kinds = ParamKinds::from_schema(SCHEMA);
    let none = ParamKinds::default();
    let at = parameter("coordinate", "[100, 246]");
    let scroll = format!(
        "{}{}",
        parameter("pixels", "-300"),
        parameter("text", "123")
    );
    let multi = parameter("text", "a\nb");
    for (what, answer, kinds, want) in [
        (
            "a click",
            xml("left_click", &at),
            &kinds,
            json!({"action": "left_click", "coordinate": [100, 246]}),
        ),
        (
            "a number by the schema, a string that looks like one stays a string",
            xml("scroll", &scroll),
            &kinds,
            json!({"action": "scroll", "pixels": -300, "text": "123"}),
        ),
        (
            "no schema: only an array or an object is JSON",
            xml("scroll", &format!("{at}{scroll}")),
            &none,
            json!({"action": "scroll", "coordinate": [100, 246], "pixels": "-300", "text": "123"}),
        ),
        (
            "a value over several lines",
            xml("type", &multi),
            &kinds,
            json!({"action": "type", "text": "a\nb"}),
        ),
        (
            "prose before the call is allowed",
            format!("I will click it.\n\n{}", xml("left_click", &at)),
            &kinds,
            json!({"action": "left_click", "coordinate": [100, 246]}),
        ),
    ] {
        let got = read(CallFormat::Qwen3CoderXml, &answer, "", kinds);
        assert_eq!(got, Ok(vec![computer_use(want)]), "{what}");
    }
}

#[test]
fn hermes_json_calls_are_read() {
    let kinds = ParamKinds::from_schema(SCHEMA);
    for (what, answer, want) in [
        (
            "arguments as an object",
            r#"<tool_call>{"name":"computer_use","arguments":{"action":"wait","time":1}}</tool_call>"#,
            Some(json!({"action": "wait", "time": 1})),
        ),
        (
            "arguments as a string holding an object",
            r#"<tool_call>{"name":"computer_use","arguments":"{\"action\":\"wait\"}"}</tool_call>"#,
            Some(json!({"action": "wait"})),
        ),
        (
            "no arguments",
            r#"<tool_call>{"name":"computer_use"}</tool_call>"#,
            Some(json!({})),
        ),
        ("not JSON", "<tool_call>{\"name\": </tool_call>", None),
        (
            "arguments that are not an object",
            r#"<tool_call>{"name":"computer_use","arguments":[1]}</tool_call>"#,
            None,
        ),
        (
            "the XML form is not the Hermes form",
            "<tool_call><function=computer_use></function></tool_call>",
            None,
        ),
    ] {
        let got = read(CallFormat::HermesJson, answer, "", &kinds);
        let want = want
            .map(|input| vec![computer_use(input)])
            .ok_or(ParseError::NoAction);
        assert_eq!(got, want, "{what}");
    }
}

#[test]
fn a_call_in_the_reasoning_counts_when_the_answer_has_none() {
    let kinds = ParamKinds::from_schema(SCHEMA);
    let first = xml("left_click", &parameter("coordinate", "[1, 2]"));
    let second = xml("left_click", &parameter("coordinate", "[3, 4]"));
    let click = |at: [u32; 2]| computer_use(json!({"action": "left_click", "coordinate": at}));
    let format = CallFormat::Qwen3CoderXml;
    // In the thought only: the last call there.
    let thought = format!("let me try {first} no, rather {second}");
    assert_eq!(read(format, "", &thought, &kinds), Ok(vec![click([3, 4])]));
    // Written inside a <think> in the answer text, and nothing after it: the last one.
    let inline = format!("<think>{first}\n{second}\n</think>");
    assert_eq!(read(format, &inline, "", &kinds), Ok(vec![click([3, 4])]));
    // A call after </think> is the answer, and the examples before it do not act.
    let answered = format!("{first}</think>\n<action>click</action>\n{second}");
    assert_eq!(read(format, &answered, "", &kinds), Ok(vec![click([3, 4])]));
    // The answer's own calls win over the thought's.
    assert_eq!(
        read(format, &second, &thought, &kinds),
        Ok(vec![click([3, 4])])
    );
}

#[test]
fn every_call_of_the_answer_is_read_up_to_the_limit() {
    let kinds = ParamKinds::from_schema(SCHEMA);
    let call = xml("wait", &parameter("time", "1"));
    let format = CallFormat::Qwen3CoderXml;
    let two = format!("{call}\n{call}");
    assert_eq!(read(format, &two, "", &kinds).unwrap().len(), 2);
    let nine = call.repeat(9);
    assert_eq!(read(format, &nine, "", &kinds).unwrap().len(), 8);
    let limits = ParseLimits {
        max_input: ByteLen(4096),
        max_actions: ActionCount(1),
    };
    let got = read_calls(format, &two, "", &kinds, limits).unwrap();
    assert_eq!(got.len(), 1);
}

#[test]
fn malformed_and_oversized_replies_are_refused() {
    let kinds = ParamKinds::from_schema(SCHEMA);
    let format = CallFormat::Qwen3CoderXml;
    let unclosed = xml("left_click", "").replace("</tool_call>", "");
    for (what, answer) in [
        ("empty", String::new()),
        ("prose", "I have finished the task.".to_owned()),
        ("a block that never closes", unclosed),
        (
            "a block with no function",
            "<tool_call>click</tool_call>".to_owned(),
        ),
        (
            "an empty function name",
            "<tool_call><function=></function></tool_call>".to_owned(),
        ),
        (
            "a name that is no name",
            "<tool_call><function=a b!></function></tool_call>".to_owned(),
        ),
        ("a closing tag only", "</tool_call>".to_owned()),
    ] {
        assert_eq!(
            read(format, &answer, "", &kinds),
            Err(ParseError::NoAction),
            "{what}"
        );
    }
    let big = format!("{}{}", xml("wait", ""), " ".repeat(33 * 1024));
    assert_eq!(read(format, &big, "", &kinds), Err(ParseError::TooLarge));
    // The bound is on the two texts together.
    let half = " ".repeat(17 * 1024);
    assert_eq!(
        read(format, &half, &half, &kinds),
        Err(ParseError::TooLarge)
    );
    // The engine reads those calls, not us.
    assert_eq!(
        read(CallFormat::Engine, &xml("wait", ""), "", &kinds),
        Err(ParseError::NoAction)
    );
}

#[test]
fn a_call_in_a_foreign_form_is_not_read() {
    // The text a model wrote inside its reasoning in the run that stalled (recorded): an
    // `<invoke>` block, which is not the format it was trained on.
    let path = format!(
        "{}/../../fixtures/ui_mate/stray_invoke.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let stray = std::fs::read_to_string(path).unwrap();
    let kinds = ParamKinds::from_schema(SCHEMA);
    for format in [CallFormat::Qwen3CoderXml, CallFormat::HermesJson] {
        assert_eq!(read(format, "", &stray, &kinds), Err(ParseError::NoAction));
        assert_eq!(
            read(format, &stray, &stray, &kinds),
            Err(ParseError::NoAction)
        );
    }
}

#[test]
fn the_wrappers_of_a_think_action_call_reply_are_read_when_they_are_there() {
    use cua_parse::{AnswerParts, answer_parts};
    let some = |think: Option<&str>, action: Option<&str>| AnswerParts {
        think: think.map(str::to_owned),
        action: action.map(str::to_owned),
    };
    for (what, text, want) in [
        (
            "all three",
            "<think>\nlook\n</think>\n<action> Click it </action>\n<tool_call></tool_call>",
            some(Some("look"), Some("Click it")),
        ),
        (
            "a template opened the block: no opening tag",
            "look</think><action>Click it</action>",
            some(Some("look"), Some("Click it")),
        ),
        (
            "case does not matter",
            "<THINK>a</THINK><Action>b</Action>",
            some(Some("a"), Some("b")),
        ),
        ("no wrappers", "just a call", some(None, None)),
        (
            "a block that never closes",
            "<think>a</think><action>b",
            some(Some("a"), None),
        ),
        (
            "empty wrappers",
            "<think> </think><action></action>",
            some(None, None),
        ),
        (
            "an action inside the reasoning is not the step's",
            "<think>I could write <action>x</action></think><action>y</action>",
            some(Some("I could write <action>x</action>"), Some("y")),
        ),
        (
            "multibyte text",
            "<think>日本語</think><action>押す</action>",
            some(Some("日本語"), Some("押す")),
        ),
    ] {
        assert_eq!(answer_parts(text), want, "{what}");
    }
    let long = format!("<think>{}</think>", "a".repeat(10_000));
    assert_eq!(answer_parts(&long).think.map(|t| t.len()), Some(4096));
}
