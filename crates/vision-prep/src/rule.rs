//! How a model wants an image resized, and what the resized image costs.

use cua_action::{Coord, DeviceSize, ImageSpace, Size};
use serde::{Deserialize, Serialize};

/// The side a Qwen-VL style model rounds an image to: 28 for Qwen2.5-VL and UI-TARS-1.5, 32
/// (patch 16 times merge 2) for the Qwen3.5 base of Holo 3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatchFactor(pub u32);

/// An area in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PixelCount(pub u64);

/// What a model charges for one image, in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageTokens(pub u32);

/// A model family's image resize rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ResizeRule {
    /// Qwen-VL family: round each side to `factor`, clamp the area, keep the aspect ratio.
    SmartResize {
        factor: PatchFactor,
        min_pixels: PixelCount,
        max_pixels: PixelCount,
    },
    /// Anthropic-style caps: a longest edge and a total area.
    LongEdge {
        max_edge: Coord,
        max_pixels: PixelCount,
    },
    /// The model sees the frame as it is.
    Identity,
}

/// Why a frame cannot be fitted to a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitError {
    #[error("the frame has no area")]
    ZeroArea,
    #[error("the frame's aspect ratio is beyond what the rule accepts")]
    AspectTooExtreme,
    #[error("the frame is below the rule's minimum")]
    BelowMinimum,
}

/// The widest aspect ratio `smart_resize` accepts (`MAX_RATIO` of qwen-vl-utils).
const MAX_RATIO: f64 = 200.0;

/// Tokens per pixel under the Anthropic-style rules: one token per 750 pixels.
const PIXELS_PER_TOKEN: u64 = 750;

/// The size the model is shown for a frame of `src` device pixels; for `SmartResize` this is
/// the reference `smart_resize` of Qwen's image processor, exactly.
///
/// The arithmetic mirrors qwen-vl-utils (Apache-2.0) in `f64`, including Python's
/// round-half-to-even, so the sizes agree with the reference on every row of
/// `fixtures/smart_resize.csv`.
pub fn fit(src: DeviceSize, rule: &ResizeRule) -> Result<Size<ImageSpace>, FitError> {
    if src.w == 0 || src.h == 0 {
        return Err(FitError::ZeroArea);
    }
    let (w, h) = match *rule {
        ResizeRule::SmartResize {
            factor,
            min_pixels,
            max_pixels,
        } => smart_resize(src, factor, min_pixels, max_pixels)?,
        ResizeRule::LongEdge {
            max_edge,
            max_pixels,
        } => long_edge(src, max_edge.0, max_pixels.0)?,
        ResizeRule::Identity => (src.w, src.h),
    };
    Ok(Size::new(Coord(w), Coord(h)))
}

fn smart_resize(
    src: DeviceSize,
    factor: PatchFactor,
    min_pixels: PixelCount,
    max_pixels: PixelCount,
) -> Result<(u32, u32), FitError> {
    if factor.0 == 0 || max_pixels.0 == 0 {
        return Err(FitError::ZeroArea);
    }
    let (w, h, f) = (f64::from(src.w), f64::from(src.h), f64::from(factor.0));
    if w.max(h) / w.min(h) > MAX_RATIO {
        return Err(FitError::AspectTooExtreme);
    }
    let round_by = |n: f64| (n / f).round_ties_even() * f;
    let floor_by = |n: f64| (n / f).floor() * f;
    let ceil_by = |n: f64| (n / f).ceil() * f;
    let (mut h_bar, mut w_bar) = (round_by(h).max(f), round_by(w).max(f));
    if h_bar * w_bar > max_pixels.0 as f64 {
        let beta = (h * w / max_pixels.0 as f64).sqrt();
        (h_bar, w_bar) = (floor_by(h / beta), floor_by(w / beta));
    } else if h_bar * w_bar < min_pixels.0 as f64 {
        let beta = (min_pixels.0 as f64 / (h * w)).sqrt();
        (h_bar, w_bar) = (ceil_by(h * beta), ceil_by(w * beta));
    }
    match (side(w_bar), side(h_bar)) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(FitError::BelowMinimum),
    }
}

/// A computed side as a pixel count: at least one, and within `u32`.
fn side(value: f64) -> Option<u32> {
    (1.0..=f64::from(u32::MAX))
        .contains(&value)
        .then_some(value as u32)
}

fn long_edge(src: DeviceSize, max_edge: u32, max_pixels: u64) -> Result<(u32, u32), FitError> {
    if max_edge == 0 || max_pixels == 0 {
        return Err(FitError::ZeroArea);
    }
    let (w, h) = (f64::from(src.w), f64::from(src.h));
    let by_edge = f64::from(max_edge) / w.max(h);
    let by_area = (max_pixels as f64 / (w * h)).sqrt();
    let scale = by_edge.min(by_area).min(1.0);
    let (mut w_out, mut h_out) = (side((w * scale).floor()), side((h * scale).floor()));
    // The float scale can overshoot the area by a hair: step both sides down until it fits.
    while let (Some(wo), Some(ho)) = (w_out, h_out) {
        if u64::from(wo) * u64::from(ho) <= max_pixels && wo.max(ho) <= max_edge {
            return Ok((wo, ho));
        }
        (w_out, h_out) = (side(f64::from(wo - 1)), side(f64::from(ho - 1)));
    }
    Err(FitError::BelowMinimum)
}

/// Tokens one image of `image` size costs under `rule`: `(h / f) * (w / f)` for `SmartResize`,
/// where the factor already carries the patch merge (28 is 14 times 2, 32 is 16 times 2), so
/// the grid it counts is the grid of merged tokens. The Anthropic-style rules and `Identity`
/// charge one token per 750 pixels. An estimate for budgeting, not the engine's own count.
pub fn image_tokens(rule: &ResizeRule, image: Size<ImageSpace>) -> ImageTokens {
    let (w, h) = (u64::from(image.w.0), u64::from(image.h.0));
    let tokens = match rule {
        ResizeRule::SmartResize { factor, .. } => {
            let f = u64::from(factor.0.max(1));
            h.div_ceil(f) * w.div_ceil(f)
        }
        ResizeRule::LongEdge { .. } | ResizeRule::Identity => (w * h).div_ceil(PIXELS_PER_TOKEN),
    };
    ImageTokens(u32::try_from(tokens).unwrap_or(u32::MAX))
}
