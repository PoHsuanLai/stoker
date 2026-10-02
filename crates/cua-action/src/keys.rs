//! Pointer buttons, modifiers, key chords and the small bounded numbers around them.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickCount {
    One,
    Two,
    Three,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollDir {
    Up,
    Down,
    Left,
    Right,
}

/// Why a key cannot be part of a chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChordError {
    #[error("the key is unidentified")]
    Unidentified,
    #[error("a dead key is not a chord key")]
    Dead,
}

/// Modifiers held while one key is pressed. The key is never `Unidentified` or `Dead`; the
/// seat maps it to a keysym.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ChordParts")]
pub struct Chord {
    pub mods: BTreeSet<Modifier>,
    pub key: keyboard_types::Key,
}

#[derive(Deserialize)]
struct ChordParts {
    mods: BTreeSet<Modifier>,
    key: keyboard_types::Key,
}

impl TryFrom<ChordParts> for Chord {
    type Error = ChordError;
    fn try_from(parts: ChordParts) -> Result<Self, ChordError> {
        Chord::new(parts.mods, parts.key)
    }
}

impl Chord {
    pub fn new(mods: BTreeSet<Modifier>, key: keyboard_types::Key) -> Result<Self, ChordError> {
        match key {
            keyboard_types::Key::Unidentified => Err(ChordError::Unidentified),
            keyboard_types::Key::Dead => Err(ChordError::Dead),
            key => Ok(Chord { mods, key }),
        }
    }
}

/// A repeat count outside `1..=20`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a key repeat is 1 to 20, got {0}")]
pub struct RepeatError(pub u8);

/// How many times a key press repeats: `1..=20`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Repeat(u8);

impl Repeat {
    pub const ONCE: Repeat = Repeat(1);
    pub const MAX: u8 = 20;

    pub fn new(times: u8) -> Result<Self, RepeatError> {
        if (1..=Self::MAX).contains(&times) {
            Ok(Repeat(times))
        } else {
            Err(RepeatError(times))
        }
    }

    pub fn times(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Repeat {
    type Error = RepeatError;
    fn try_from(times: u8) -> Result<Self, RepeatError> {
        Repeat::new(times)
    }
}

impl From<Repeat> for u8 {
    fn from(repeat: Repeat) -> u8 {
        repeat.0
    }
}

/// Wheel notches for one scroll action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Notches(pub u16);

/// A wait over the 60 s a parser accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a wait is at most {max} ms, got {0}", max = WaitMs::MAX)]
pub struct WaitError(pub u32);

/// Milliseconds to wait, at most [`WaitMs::MAX`]. Policy clamps further.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct WaitMs(u32);

impl WaitMs {
    pub const MAX: u32 = 60_000;

    pub fn new(ms: u32) -> Result<Self, WaitError> {
        if ms <= Self::MAX {
            Ok(WaitMs(ms))
        } else {
            Err(WaitError(ms))
        }
    }

    pub fn ms(&self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for WaitMs {
    type Error = WaitError;
    fn try_from(ms: u32) -> Result<Self, WaitError> {
        WaitMs::new(ms)
    }
}

impl From<WaitMs> for u32 {
    fn from(wait: WaitMs) -> u32 {
        wait.0
    }
}
