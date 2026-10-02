//! How loud a frame is, for the mic indicator and the orb.

use serde::{Deserialize, Serialize};
use speech_provider::Frame512;

/// Loudness in thousandths: RMS in dBFS clamped to -60..0 and mapped to 0..=1000.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Level(pub u16);

/// The level of one frame. Integer arithmetic only (no floats anywhere in stoker).
pub fn level_of(frame: &Frame512) -> Level {
    let _ = frame;
    todo!("level_of: integer RMS, dBFS from a table, clamp -60..0 to 0..=1000")
}
