//! The charge of an OpenRouter reply: `usage.cost` in the last chunk, read exactly, and only for
//! that flavor. The fixture is built from OpenRouter's documented response shape, not recorded.

use model_openai_compat::{Flavor, StreamDecoder};
use model_provider::{ModelName, Money, Tokens, TurnEvent, TurnUsage};
use model_wire::ChatDecoder;
use rust_decimal::Decimal;

const FROM_DOCS: &str = include_str!("../fixtures/openrouter_stream_cost_from_docs.sse");

fn usage_of(flavor: Flavor, frames: &[&str]) -> TurnUsage {
    let mut decoder = StreamDecoder::new(flavor, ModelName("m".into()));
    let mut last = TurnUsage::default();
    for frame in frames {
        for event in decoder.feed(frame).unwrap() {
            if let TurnEvent::Usage(usage) = event {
                last = usage;
            }
        }
    }
    last
}

fn with_cost(cost: &str) -> String {
    format!(r#"{{"choices":[],"usage":{{"prompt_tokens":5,"completion_tokens":1,"cost":{cost}}}}}"#)
}

fn charge_of(flavor: Flavor, cost: &str) -> Option<Money> {
    usage_of(flavor, &[&with_cost(cost)]).charge
}

#[test]
fn the_documented_stream_ends_with_its_exact_charge() {
    let frames: Vec<&str> = FROM_DOCS.lines().collect();
    let usage = usage_of(Flavor::OpenRouter, &frames);
    assert_eq!(usage.input, Tokens(194));
    assert_eq!(usage.output, Tokens(2));
    assert_eq!(usage.charge, Some(Money::usd(Decimal::new(95, 5))));
}

#[test]
fn only_the_openrouter_flavor_reads_a_charge() {
    for flavor in [Flavor::LlamaServer, Flavor::Vllm, Flavor::LiteLlm] {
        assert_eq!(charge_of(flavor, "0.5"), None, "{flavor:?}");
    }
    assert_eq!(
        charge_of(Flavor::OpenRouter, "0.5"),
        Some(Money::usd(Decimal::new(5, 1)))
    );
}

#[test]
fn the_number_is_read_from_its_text() {
    let cases = [
        ("0.1", Decimal::new(1, 1)),
        ("0.000001234567", Decimal::new(1_234_567, 12)),
        ("1.4e-7", Decimal::new(14, 8)),
        ("0", Decimal::ZERO),
        ("3", Decimal::from(3)),
    ];
    for (text, want) in cases {
        assert_eq!(
            charge_of(Flavor::OpenRouter, text),
            Some(Money::usd(want)),
            "{text}"
        );
    }
}

#[test]
fn a_charge_that_cannot_be_read_is_none_and_the_tokens_stay() {
    for text in ["-0.5", "null", r#""0.5""#, "true", "[1]", "{}", "1e40"] {
        let usage = usage_of(Flavor::OpenRouter, &[&with_cost(text)]);
        assert_eq!(usage.charge, None, "{text}");
        assert_eq!(usage.input, Tokens(5), "{text}");
    }
}

#[test]
fn a_usage_chunk_without_cost_has_no_charge() {
    let frame = r#"{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":1}}"#;
    assert_eq!(usage_of(Flavor::OpenRouter, &[frame]).charge, None);
}
