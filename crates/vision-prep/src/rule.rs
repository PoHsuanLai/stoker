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

/// The size the model is shown for a frame of `src` device pixels; for `SmartResize` this is
/// the reference `smart_resize` of Qwen's image processor, exactly.
pub fn fit(src: DeviceSize, rule: &ResizeRule) -> Result<Size<ImageSpace>, FitError> {
    let _ = (src, rule);
    todo!("fit: smart_resize, long-edge and identity rules")
}

/// Tokens one image of `image` size costs under `rule`: `(h / f) * (w / f) / merge` for
/// `SmartResize`.
pub fn image_tokens(rule: &ResizeRule, image: Size<ImageSpace>) -> ImageTokens {
    let _ = (rule, image);
    todo!("image_tokens: per-rule token estimate")
}
