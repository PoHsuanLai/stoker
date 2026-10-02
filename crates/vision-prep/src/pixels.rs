//! Raw frames in, one encoded image out.

use cua_action::{DeviceSize, ImageSpace, PixelFormat, Size};
use serde::{Deserialize, Serialize};

#[cfg(feature = "pixels")]
use crate::FrameMap;

/// A captured frame, borrowed from the capturer's buffer.
#[derive(Debug, Clone, Copy)]
pub struct RawFrame<'a> {
    pub pixels: &'a [u8],
    pub size: DeviceSize,
    pub stride: u32,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Png,
    Jpeg,
}

/// JPEG quality, `1..=100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct JpegQuality(u8);

/// A JPEG quality outside `1..=100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("JPEG quality is 1 to 100, got {0}")]
pub struct JpegQualityError(pub u8);

impl JpegQuality {
    pub fn new(quality: u8) -> Result<Self, JpegQualityError> {
        if (1..=100).contains(&quality) {
            Ok(Self(quality))
        } else {
            Err(JpegQualityError(quality))
        }
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for JpegQuality {
    type Error = JpegQualityError;
    fn try_from(quality: u8) -> Result<Self, JpegQualityError> {
        JpegQuality::new(quality)
    }
}

impl From<JpegQuality> for u8 {
    fn from(quality: JpegQuality) -> u8 {
        quality.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Encoding {
    Png,
    Jpeg(JpegQuality),
}

/// An image ready to send: encoded once, at the size the model is shown.
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedImage {
    pub media: MediaType,
    pub bytes: Vec<u8>,
    pub size: Size<ImageSpace>,
}

// The bytes are a screenshot: Debug shows their length only.
impl core::fmt::Debug for PreparedImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PreparedImage")
            .field("media", &self.media)
            .field("bytes", &format_args!("<{} bytes>", self.bytes.len()))
            .field("size", &self.size)
            .finish()
    }
}

/// Why a frame could not be prepared.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepError {
    #[error("the buffer is shorter than its size and stride say")]
    ShortBuffer,
    #[error("the frame's size does not match the map's window")]
    SizeMismatch,
    #[error("the encoder failed")]
    Encode,
}

/// Resizes once and encodes once, from the frame's pixel format to `enc`.
#[cfg(feature = "pixels")]
pub fn prepare(
    raw: RawFrame<'_>,
    map: &FrameMap,
    enc: Encoding,
) -> Result<PreparedImage, PrepError> {
    let _ = (raw, map, enc);
    todo!("prepare: fast_image_resize then image encode")
}
