//! What a model is shown of its actions: the common computer-use schema, with the verbs of the
//! model's action profile. A fine-tuned model does best when it is shown the verbs it was trained
//! on, so the action enum and the lines that describe each verb follow the profile (`finished`
//! for `terminate`, `hotkey` and `press` for `key`, no `hscroll` when `scroll` has a direction).
//! The profile is data from the catalogue entry; no prompt file exists per model.
//!
//! The schema of the default profile is the file itself, unchanged.

use std::borrow::Cow;

use cua_action::{ActionProfile, DragStyle, ScrollAxis, VerbAlias};
use serde_json::{Map, Value, json};

/// A drag that presses at the cursor.
const DRAG_FROM_CURSOR: &str = "press the left button where the cursor is, move to `coordinate` while holding it, release there: moves, resizes, selects a range, draws a shape or line";
/// A scroll whose axis is the `direction` argument.
const SCROLL_BY_DIRECTION: &str = "scroll at `coordinate` by `pixels` along `direction` (positive is up or right, negative is down or left)";
const CALL_USER: &str = "ask the person the question in `text` and wait for the answer";
const PIXELS_BY_DIRECTION: &str = "How far `scroll` scrolls; positive is up or right.";

/// The schema text to declare for a model: `base` with the profile's verbs.
pub(crate) fn schema(base: &str, profile: ActionProfile) -> Cow<'_, str> {
    if profile.is_plain() {
        return Cow::Borrowed(base);
    }
    match serde_json::from_str::<Value>(base) {
        Ok(mut doc) => {
            if let Some(properties) = doc.get_mut("properties").and_then(Value::as_object_mut) {
                rewrite(properties, profile);
            }
            Cow::Owned(doc.to_string())
        }
        Err(_) => Cow::Borrowed(base),
    }
}

fn rewrite(properties: &mut Map<String, Value>, profile: ActionProfile) {
    if let Some(action) = properties.get_mut("action").and_then(Value::as_object_mut) {
        rewrite_action(action, profile);
    }
    for (_, property) in properties
        .iter_mut()
        .filter(|(name, _)| name.as_str() != "action")
    {
        if let Some(Value::String(text)) = property.get_mut("description") {
            *text = mentions(profile, text);
        }
    }
    match profile.scroll {
        ScrollAxis::ByVerb => {}
        ScrollAxis::ByDirection => {
            properties.insert(
                "direction".to_owned(),
                json!({
                    "type": "string",
                    "enum": ["vertical", "horizontal"],
                    "description": "The axis `scroll` moves along; `vertical` when left out."
                }),
            );
            properties.insert(
                "pixels".to_owned(),
                json!({ "type": "integer", "description": PIXELS_BY_DIRECTION }),
            );
        }
    }
    match profile.drag {
        DragStyle::FromStart => {}
        DragStyle::FromCursor => {
            properties.remove("start_coordinate");
        }
    }
}

/// The verb names of the `enum` and the lines of the description, by the profile.
fn rewrite_action(action: &mut Map<String, Value>, profile: ActionProfile) {
    let base: Vec<String> = action
        .get("enum")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    // A verb the profile adds to the convention: it has no verb of the base to replace.
    let added: Vec<VerbAlias> = profile
        .aliases
        .iter()
        .filter(|alias| !base.iter().any(|name| name == alias.canonical()))
        .collect();
    let names: Vec<String> = base
        .iter()
        .flat_map(|name| shown_names(profile, name))
        .chain(added.iter().map(|alias| alias.model_verb().to_owned()))
        .collect();
    action.insert("enum".to_owned(), json!(names));
    if let Some(Value::String(text)) = action.get_mut("description") {
        let lines = text
            .lines()
            .flat_map(|line| shown_lines(profile, line))
            .chain(added.iter().map(|alias| added_line(*alias)));
        *text = lines.collect::<Vec<String>>().join("\n");
    }
}

/// The names a model is shown for one verb of the convention: its aliases, or the verb itself.
/// `hscroll` is not shown to a model whose `scroll` takes a direction.
fn shown_names(profile: ActionProfile, canonical: &str) -> Vec<String> {
    let aliases: Vec<String> = profile
        .aliases
        .of(canonical)
        .map(|alias| alias.model_verb().to_owned())
        .collect();
    match (aliases.is_empty(), canonical, profile.scroll) {
        (_, "hscroll", ScrollAxis::ByDirection) => Vec::new(),
        (true, _, _) => vec![canonical.to_owned()],
        (false, _, _) => aliases,
    }
}

/// One line of the action description (`* \`verb\`: what it does`) as the profile shows it; a
/// line that is no verb's stays.
fn shown_lines(profile: ActionProfile, line: &str) -> Vec<String> {
    let Some((canonical, said)) = line
        .strip_prefix("* `")
        .and_then(|rest| rest.split_once("`: "))
    else {
        return vec![line.to_owned()];
    };
    let said = match (canonical, profile.drag, profile.scroll) {
        ("left_click_drag", DragStyle::FromCursor, _) => DRAG_FROM_CURSOR,
        ("scroll", _, ScrollAxis::ByDirection) => SCROLL_BY_DIRECTION,
        _ => said,
    };
    shown_names(profile, canonical)
        .into_iter()
        .map(|name| format!("* `{name}`: {said}"))
        .collect()
}

fn added_line(alias: VerbAlias) -> String {
    let said = match alias {
        VerbAlias::CallUser => CALL_USER,
        VerbAlias::Drag | VerbAlias::Finished | VerbAlias::Hotkey | VerbAlias::Press => "",
    };
    format!("* `{}`: {said}", alias.model_verb())
}

/// A property's description, with each verb of the convention that it names (`terminate`, `key`)
/// written as the verbs the model is shown.
fn mentions(profile: ActionProfile, text: &str) -> String {
    profile.aliases.iter().map(|alias| alias.canonical()).fold(
        text.to_owned(),
        |text, canonical| {
            let shown: Vec<String> = shown_names(profile, canonical)
                .iter()
                .map(|name| format!("`{name}`"))
                .collect();
            text.replace(&format!("`{canonical}`"), &shown.join(" or "))
        },
    )
}
