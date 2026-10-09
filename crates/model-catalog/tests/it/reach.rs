//! `reachable`: which of a hosted entry's reaches to use.

use crate::support;

use model_catalog::{
    CatalogError, Locality, ModelEntry, ProviderId, Reach, Wire, parse_entry, reachable,
};
use support::*;

fn id(s: &str) -> ProviderId {
    ProviderId(s.into())
}

/// A hosted entry with the three reaches a Claude model has in the shipped catalogue shape:
/// direct on the Messages wire, a second direct one, and the gateway.
fn hosted(reaches: &[(&str, &str)]) -> ModelEntry {
    let reach: Vec<String> = reaches
        .iter()
        .map(|(provider, wire)| {
            format!(
                r#"{{ provider = "{provider}", model = "m", price = {{ input_per_mtok = 1, output_per_mtok = 2 }}, wire = "{wire}" }}"#
            )
        })
        .collect();
    let locality = format!(
        r#"locality = {{ kind = "remote", v = {{ reach = [{}] }} }}"#,
        reach.join(",")
    );
    let text = file("h", r#"["text"]"#, r#"["text"]"#, &[locality, text_out(false)], "")
        .replace(r#"source = { kind = "hugging_face", v = { repo = "a/b", revision = "0123456789abcdef0123456789abcdef01234567" } }"#, r#"source = { kind = "hosted" }"#)
        .replace(r#"licence = { kind = "open", v = "Apache-2.0" }"#, r#"licence = { kind = "proprietary" }"#);
    parse_entry(&text).unwrap()
}

fn chosen<'a>(entry: &'a ModelEntry, granted: &[&str], wires: &[Wire]) -> Option<&'a str> {
    let granted: Vec<ProviderId> = granted.iter().map(|p| id(p)).collect();
    reachable(entry, &granted, wires).map(|r| r.provider.0.as_str())
}

const BOTH: [Wire; 2] = [Wire::OpenAiCompat, Wire::AnthropicMessages];
const OPEN: [Wire; 1] = [Wire::OpenAiCompat];

#[test]
fn a_direct_reach_beats_the_gateway_when_both_are_granted() {
    let e = hosted(&[
        ("openrouter", "open_ai_compat"),
        ("openai", "open_ai_compat"),
    ]);
    assert_eq!(chosen(&e, &["openrouter", "openai"], &OPEN), Some("openai"));
    assert_eq!(chosen(&e, &["openai", "openrouter"], &OPEN), Some("openai"));
}

#[test]
fn the_gateway_serves_when_it_is_all_that_is_granted() {
    let e = hosted(&[
        ("openai", "open_ai_compat"),
        ("openrouter", "open_ai_compat"),
    ]);
    let reach = reachable(&e, &[id("openrouter")], &OPEN).unwrap();
    assert!(reach.is_via_gateway());
    assert_eq!(reach.model.0, "m");
    assert_eq!(chosen(&e, &["openai"], &OPEN), Some("openai"));
}

#[test]
fn nothing_granted_or_nothing_matching_gives_none() {
    let e = hosted(&[
        ("openai", "open_ai_compat"),
        ("openrouter", "open_ai_compat"),
    ]);
    assert_eq!(chosen(&e, &[], &OPEN), None);
    assert_eq!(chosen(&e, &["moonshot"], &OPEN), None);
    assert_eq!(chosen(&e, &["openai", "openrouter"], &[]), None);
}

#[test]
fn a_wire_this_build_cannot_speak_is_skipped() {
    // Claude: direct on the Messages wire, the gateway on the OpenAI-compatible one.
    let claude = hosted(&[
        ("anthropic", "anthropic_messages"),
        ("openrouter", "open_ai_compat"),
    ]);
    let granted = ["anthropic", "openrouter"];
    // Today's build speaks only the OpenAI-compatible wire: Claude resolves via OpenRouter.
    assert_eq!(chosen(&claude, &granted, &OPEN), Some("openrouter"));
    // Once a Messages adapter lands the direct reach wins.
    assert_eq!(chosen(&claude, &granted, &BOTH), Some("anthropic"));
    // Anthropic granted alone: nothing it can speak yet.
    assert_eq!(chosen(&claude, &["anthropic"], &OPEN), None);
    assert_eq!(chosen(&claude, &["anthropic"], &BOTH), Some("anthropic"));
}

#[test]
fn ties_take_the_first_listed() {
    let e = hosted(&[("moonshot", "open_ai_compat"), ("openai", "open_ai_compat")]);
    assert_eq!(chosen(&e, &["openai", "moonshot"], &OPEN), Some("moonshot"));
}

#[test]
fn an_on_device_entry_has_no_reach() {
    let local = parse_entry(&text_only("t")).unwrap();
    assert_eq!(local.locality, Locality::OnDevice);
    assert_eq!(chosen(&local, &["openai", "openrouter"], &BOTH), None);
}

#[test]
fn the_choice_comes_back_whole_for_the_footer_and_the_price() {
    let e = hosted(&[("openrouter", "open_ai_compat")]);
    let reach: &Reach = reachable(&e, &[id("openrouter")], &OPEN).unwrap();
    assert_eq!(reach.price.input_per_mtok.0, 1);
    assert_eq!(reach.price.output_per_mtok.0, 2);
    assert_eq!(reach.wire, Wire::OpenAiCompat);
}

#[test]
fn a_remote_entry_needs_reaches_and_no_engine() {
    let text = file(
        "h",
        r#"["text"]"#,
        r#"["text"]"#,
        &[r#"locality = { kind = "remote", v = { reach = [] } }"#.into(), text_out(false)],
        "",
    )
    .replace(r#"source = { kind = "hugging_face", v = { repo = "a/b", revision = "0123456789abcdef0123456789abcdef01234567" } }"#, r#"source = { kind = "hosted" }"#);
    assert_eq!(parse_entry(&text), Err(CatalogError::RemoteWithoutReach));
    let with_engine = hosted(&[("openai", "open_ai_compat")]);
    let mut text = toml::to_string(&with_engine).unwrap();
    text.push_str(ENGINE);
    assert_eq!(parse_entry(&text), Err(CatalogError::RemoteWithEngines));
}
