//! An attached entry: served elsewhere, no engine profile, still a slot member by its wire.

use model_catalog::{CatalogError, EngineKind, ModelEntry, Slot, parse_entry, slot_members};

const ATTACHED: &str = include_str!("../../../catalog/qwen3.5-35b-a3b-fp8.toml");
const ENGINE: &str =
    "\n[[engine]]\nkind = \"vllm\"\nargs = []\nweights = { kind = \"hf_snapshot\" }\n";

fn entry(text: &str) -> ModelEntry {
    parse_entry(text).unwrap()
}

#[test]
fn an_attached_entry_lists_no_engine() {
    let text = format!("{ATTACHED}{ENGINE}");
    assert_eq!(parse_entry(&text), Err(CatalogError::AttachedWithEngines));
}

#[test]
fn an_attached_entry_is_not_remote() {
    let text = ATTACHED.replace(
        "inputs = [",
        "locality = { kind = \"remote\", v = { reach = [] } }\ninputs = [",
    );
    assert_eq!(parse_entry(&text), Err(CatalogError::AttachedRemote));
}

#[test]
fn a_launched_entry_still_needs_an_engine() {
    let text = ATTACHED
        .lines()
        .filter(|l| !l.starts_with("serving"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse_entry(&text), Err(CatalogError::NoEngine));
}

#[test]
fn an_attached_entry_serves_a_slot_only_through_a_wire_the_caller_has() {
    let catalogue = [entry(ATTACHED)];
    let members = |engines: &[EngineKind]| slot_members(Slot::Text, &catalogue, engines).len();
    assert_eq!(members(&[EngineKind::Vllm]), 1);
    assert_eq!(members(&[EngineKind::LlamaServer]), 0);
    assert_eq!(members(&[]), 0);
}

#[test]
fn the_parsers_are_required_but_the_reasoning_parser_is_optional() {
    let without = ATTACHED.replace(", reasoning_parser = \"qwen3\"", "");
    assert_eq!(entry(&without).serving, {
        let mut e = entry(ATTACHED).serving;
        if let model_catalog::Serving::Attached(a) = &mut e {
            a.reasoning_parser = None;
        }
        e
    });
    let no_tool = ATTACHED.replace("tool_parser = \"qwen3_coder\", ", "");
    assert!(matches!(parse_entry(&no_tool), Err(CatalogError::Toml(_))));
}
