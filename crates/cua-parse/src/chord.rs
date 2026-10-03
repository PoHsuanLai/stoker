//! Key chords from the words a model writes: `ctrl c`, `ctrl+shift+t`, `Enter`, `["alt", "Tab"]`.

use std::collections::BTreeSet;
use std::str::FromStr;

use cua_action::{Chord, Modifier};
use keyboard_types::Key;

use crate::DropReason;

/// A chord from the words of one hotkey string: split on whitespace when there is any, else on
/// `+` (a lone `+` is the plus key).
pub(crate) fn from_text(text: &str) -> Result<Chord, DropReason> {
    let words: Vec<&str> = if text.split_whitespace().nth(1).is_some() {
        text.split_whitespace().collect()
    } else if text.trim() == "+" {
        vec!["+"]
    } else {
        text.split('+').filter(|w| !w.is_empty()).collect()
    };
    from_words(&words)
}

/// A chord from key names: every modifier name joins `mods`, and exactly one other key is the
/// key. A lone modifier is pressed as the key itself (the Super key opens an overview).
pub(crate) fn from_words<S: AsRef<str>>(words: &[S]) -> Result<Chord, DropReason> {
    let mut mods = BTreeSet::new();
    let mut keys = Vec::new();
    for word in words {
        match modifier(word.as_ref()) {
            Some(m) => {
                mods.insert(m);
            }
            None => keys.push(word.as_ref()),
        }
    }
    let key = match (keys.as_slice(), words) {
        ([key], _) => named(key),
        ([], [only]) => modifier(only.as_ref()).map(modifier_key),
        _ => None,
    };
    let key = key.ok_or(if words.is_empty() {
        DropReason::MissingArgument
    } else {
        DropReason::BadArgument
    })?;
    let mods = if keys.is_empty() {
        BTreeSet::new()
    } else {
        mods
    };
    Chord::new(mods, key).map_err(|_| DropReason::BadArgument)
}

fn modifier(word: &str) -> Option<Modifier> {
    match word.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifier::Ctrl),
        "alt" | "option" | "opt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "super" | "cmd" | "command" | "win" | "windows" | "meta" => Some(Modifier::Super),
        _ => None,
    }
}

fn modifier_key(m: Modifier) -> Key {
    match m {
        Modifier::Ctrl => Key::Control,
        Modifier::Alt => Key::Alt,
        Modifier::Shift => Key::Shift,
        Modifier::Super => Key::Super,
    }
}

/// A key by the name a model is likely to write: the aliases first, one character as itself,
/// then the W3C names (`ArrowUp`, `F5`) in either case.
fn named(word: &str) -> Option<Key> {
    let lower = word.to_ascii_lowercase();
    let w3c = match lower.as_str() {
        "enter" | "return" => "Enter",
        "esc" | "escape" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" | "del" => "Delete",
        "insert" | "ins" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" | "pgup" => "PageUp",
        "pagedown" | "pgdn" => "PageDown",
        "up" | "arrowup" => "ArrowUp",
        "down" | "arrowdown" => "ArrowDown",
        "left" | "arrowleft" => "ArrowLeft",
        "right" | "arrowright" => "ArrowRight",
        "capslock" => "CapsLock",
        "space" | "spacebar" => return Some(Key::Character(" ".into())),
        _ => "",
    };
    if !w3c.is_empty() {
        return Key::from_str(w3c).ok();
    }
    let mut chars = word.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(Key::Character(word.to_owned())),
        _ => function_key(&lower),
    }
}

/// `f1` to `f24`.
fn function_key(lower: &str) -> Option<Key> {
    let n: u8 = lower.strip_prefix('f')?.parse().ok()?;
    (1..=24)
        .contains(&n)
        .then(|| Key::from_str(&format!("F{n}")).ok())?
}
