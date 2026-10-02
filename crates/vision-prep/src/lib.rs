//! Image preparation for vision models: how big the model should see a frame, what that costs
//! in tokens, and the map from the model's coordinates back to the window's.

mod frame_map;
mod pixels;
mod rule;

pub use frame_map::{FrameMap, MapError};
#[cfg(feature = "pixels")]
pub use pixels::prepare;
pub use pixels::{
    Encoding, JpegQuality, JpegQualityError, MediaType, PrepError, PreparedImage, RawFrame,
};
pub use rule::{FitError, ImageTokens, PatchFactor, PixelCount, ResizeRule, fit, image_tokens};
