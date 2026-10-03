//! The tool declarations and the result encodings.

use cua_action::{Coord, Size, WireDialect};
use cua_vendors::{StepResult, WireCodec, codec};
use model_provider::{ImageInput, Part, ToolCallId, ToolSpec, ToolStatus};

fn image() -> ImageInput {
    serde_json::from_value(serde_json::json!({
        "media": "png", "bytes": "AAEC", "detail": "auto"
    }))
    .unwrap()
}

fn id(s: &str) -> ToolCallId {
    ToolCallId(s.into())
}

fn config(dialect: WireDialect) -> String {
    let tools = codec(dialect).tools(Size::new(Coord(1024), Coord(768)));
    let [ToolSpec::Native(native)] = &tools[..] else {
        panic!("one native tool")
    };
    assert_eq!(native.dialect, dialect);
    native.config.as_str().to_owned()
}

#[test]
fn each_dialect_declares_its_vendors_tool() {
    assert_eq!(
        config(WireDialect::AnthropicToolset20260801),
        r#"{"type":"computer_toolset_20260801"}"#
    );
    let legacy: serde_json::Value =
        serde_json::from_str(&config(WireDialect::AnthropicComputer20251124)).unwrap();
    assert_eq!(
        legacy,
        serde_json::json!({"type":"computer_20251124","name":"computer",
            "display_width_px":1024,"display_height_px":768,"enable_zoom":true})
    );
    assert_eq!(
        config(WireDialect::OpenAiComputer),
        r#"{"type":"computer"}"#
    );
    let gemini: serde_json::Value =
        serde_json::from_str(&config(WireDialect::GeminiComputerUse)).unwrap();
    assert_eq!(
        gemini,
        serde_json::json!({"type":"computer_use","environment":"desktop"})
    );
}

fn results(part: &Part) -> (&ToolCallId, ToolStatus, Vec<String>, usize) {
    let Part::ToolResult(r) = part else {
        panic!("a tool result")
    };
    let texts = r
        .parts
        .iter()
        .filter_map(|p| {
            if let Part::Text(t) = p {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect();
    let images = r
        .parts
        .iter()
        .filter(|p| matches!(p, Part::Image(_)))
        .count();
    (&r.id, r.status, texts, images)
}

#[test]
fn anthropic_batch_stops_at_first_failure() {
    let next = image();
    for dialect in [
        WireDialect::AnthropicToolset20260801,
        WireDialect::AnthropicComputer20251124,
    ] {
        // The caller reports a call done after a refusal: the codec still says it did not run.
        let done = [
            StepResult::Done(id("a")),
            StepResult::Refused {
                id: id("b"),
                why: "outside the lease\n".into(),
            },
            StepResult::Done(id("c")),
            StepResult::NotRun(id("d")),
        ];
        let parts = codec(dialect).results(&done, &next);
        let (a, b, c, d) = (
            results(&parts[0]),
            results(&parts[1]),
            results(&parts[2]),
            results(&parts[3]),
        );
        assert_eq!(
            (a.0, a.1, a.2.as_slice()),
            (&id("a"), ToolStatus::Ok, &["OK".to_owned()][..])
        );
        assert_eq!(
            (b.1, b.2.as_slice()),
            (
                ToolStatus::Error,
                &["Error: outside the lease".to_owned()][..]
            )
        );
        let halt = "Not executed: an earlier computer action in this turn failed.".to_owned();
        assert_eq!((c.1, c.2), (ToolStatus::Error, vec![halt.clone()]));
        assert_eq!((d.1, d.2), (ToolStatus::Error, vec![halt]));
        // The last call did not run, so the screenshot follows the results.
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[4], Part::Image(next.clone()));
        assert!(parts[..4].iter().all(|p| results(p).3 == 0));
    }
}

#[test]
fn anthropic_puts_the_screenshot_in_the_last_result_that_ran() {
    let next = image();
    let codec = codec(WireDialect::AnthropicToolset20260801);
    let parts = codec.results(
        &[StepResult::Done(id("a")), StepResult::Done(id("b"))],
        &next,
    );
    assert_eq!(parts.len(), 2);
    assert_eq!(results(&parts[0]).3, 0);
    assert_eq!(results(&parts[1]).3, 1);
    assert_eq!(codec.results(&[], &next), vec![Part::Image(next.clone())]);
}

#[test]
fn openai_and_gemini_return_a_screenshot_with_every_result() {
    let next = image();
    for dialect in [WireDialect::OpenAiComputer, WireDialect::GeminiComputerUse] {
        let done = [
            StepResult::Done(id("a")),
            StepResult::Refused {
                id: id("b"),
                why: "no".into(),
            },
        ];
        let parts = codec(dialect).results(&done, &next);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| results(p).3 == 1));
        assert_eq!(results(&parts[1]).1, ToolStatus::Error);
        assert_eq!(
            codec(dialect).results(&[], &next),
            vec![Part::Image(next.clone())]
        );
    }
}
