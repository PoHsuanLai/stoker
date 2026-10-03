//! How loud a frame is, for the mic indicator and the orb.

use serde::{Deserialize, Serialize};
use speech_provider::Frame512;

/// Loudness in thousandths: RMS in dBFS clamped to -60..0 and mapped to 0..=1000 (-40 dBFS is 333).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Level(pub u16);

/// The level of one frame. Integer arithmetic only (no floats anywhere in stoker).
pub fn level_of(frame: &Frame512) -> Level {
    let energy: u64 = frame
        .samples()
        .iter()
        .map(|s| u64::from(s.unsigned_abs()).pow(2))
        .sum();
    level_of_energy(energy)
}

/// Binary fraction bits of the fixed-point logarithm.
const FRACTION_BITS: u32 = 16;
/// `10 * log10(2)` in millionths: a power ratio's decibels from its base-2 logarithm.
const MICRO_DB_PER_OCTAVE_OF_POWER: i64 = 3_010_300;
/// Decibels (in thousandths) the scale spans below full scale.
const SPAN_MILLI_DB: i64 = 60_000;
/// The sum of squares of 512 samples at full scale (`32768`) is `2^39`: `log2(sum) - 39` is the
/// level below full scale, in octaves of power.
const FULL_SCALE_OCTAVES: i64 = 39;

/// The level of a frame from its sum of squared samples.
fn level_of_energy(energy: u64) -> Level {
    if energy == 0 {
        return Level(0);
    }
    let octaves = i64::from(log2_fixed(energy)) - (FULL_SCALE_OCTAVES << FRACTION_BITS);
    // Thousandths of a decibel, at most 0 (a full-scale frame); the sum is at most 2^39 so the
    // product cannot overflow.
    let milli_db = octaves * MICRO_DB_PER_OCTAVE_OF_POWER / (1000 << FRACTION_BITS);
    let above_floor = (SPAN_MILLI_DB + milli_db).clamp(0, SPAN_MILLI_DB);
    let level = (above_floor * 1000 + SPAN_MILLI_DB / 2) / SPAN_MILLI_DB;
    Level(u16::try_from(level).unwrap_or(1000))
}

/// `log2(x)` of a nonzero integer in fixed point with [`FRACTION_BITS`] fraction bits, by
/// squaring the mantissa one bit at a time. Integer arithmetic only.
fn log2_fixed(x: u64) -> u32 {
    const ONE: u64 = 1 << 30;
    let whole = 63 - x.leading_zeros();
    // The mantissa in [1, 2), as a 30-bit fixed-point number.
    let mut mantissa = if whole >= 30 {
        x >> (whole - 30)
    } else {
        x << (30 - whole)
    };
    let mut fraction = 0_u32;
    for _ in 0..FRACTION_BITS {
        mantissa = (mantissa * mantissa) >> 30;
        fraction <<= 1;
        if mantissa >= 2 * ONE {
            fraction |= 1;
            mantissa >>= 1;
        }
    }
    (whole << FRACTION_BITS) | fraction
}
