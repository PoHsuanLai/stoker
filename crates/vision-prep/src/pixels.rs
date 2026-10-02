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
///
/// The frame is read as opaque RGB (the alpha byte of `Argb8888` and `Abgr8888` is dropped: a
/// window capture has no backdrop to blend with), resized with fast_image_resize's default
/// Lanczos3 filter when the map's image size differs from the frame, and encoded by `image`.
#[cfg(feature = "pixels")]
pub fn prepare(
    raw: RawFrame<'_>,
    map: &FrameMap,
    enc: Encoding,
) -> Result<PreparedImage, PrepError> {
    check_geometry(&raw, map)?;
    let rgb = to_rgb(&raw)?;
    let (w, h) = (map.image.w.0, map.image.h.0);
    let rgb = if (raw.size.w, raw.size.h) == (w, h) {
        rgb
    } else {
        resize_rgb(rgb, raw.size, (w, h))?
    };
    let (media, bytes) = encode(&rgb, (w, h), enc)?;
    Ok(PreparedImage {
        media,
        bytes,
        size: map.image,
    })
}

/// Bytes per pixel of every `wl_shm` format a capturer hands over.
#[cfg(feature = "pixels")]
const BYTES_PER_PIXEL: usize = 4;

/// The frame must be a nonzero size that agrees with the window to within the rounding of the
/// scale (a device size is the logical size times scale, rounded), and the buffer must hold it.
#[cfg(feature = "pixels")]
fn check_geometry(raw: &RawFrame<'_>, map: &FrameMap) -> Result<(), PrepError> {
    let (w, h) = (raw.size.w as usize, raw.size.h as usize);
    let (window_w, window_h) = (u64::from(map.window.w.0), u64::from(map.window.h.0));
    let skew = (u64::from(raw.size.w) * window_h).abs_diff(u64::from(raw.size.h) * window_w);
    let empty = w == 0 || h == 0 || map.image.w.0 == 0 || map.image.h.0 == 0;
    if empty || skew > window_w + window_h {
        return Err(PrepError::SizeMismatch);
    }
    let row = w
        .checked_mul(BYTES_PER_PIXEL)
        .ok_or(PrepError::ShortBuffer)?;
    let stride = raw.stride as usize;
    let needed = (h - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(row))
        .ok_or(PrepError::ShortBuffer)?;
    if stride < row || raw.pixels.len() < needed {
        return Err(PrepError::ShortBuffer);
    }
    Ok(())
}

/// Where red, green and blue sit in one four-byte pixel (little-endian `wl_shm` words).
#[cfg(feature = "pixels")]
fn channel_order(format: PixelFormat) -> [usize; 3] {
    match format {
        PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => [2, 1, 0],
        PixelFormat::Xbgr8888 | PixelFormat::Abgr8888 => [0, 1, 2],
    }
}

#[cfg(feature = "pixels")]
fn to_rgb(raw: &RawFrame<'_>) -> Result<Vec<u8>, PrepError> {
    let [r, g, b] = channel_order(raw.format);
    let (w, h, stride) = (
        raw.size.w as usize,
        raw.size.h as usize,
        raw.stride as usize,
    );
    let rows = raw.pixels.chunks(stride).take(h);
    Ok(rows
        .flat_map(|row| {
            row[..w * BYTES_PER_PIXEL]
                .as_chunks::<BYTES_PER_PIXEL>()
                .0
                .iter()
        })
        .flat_map(|px| [px[r], px[g], px[b]])
        .collect())
}

#[cfg(feature = "pixels")]
fn resize_rgb(rgb: Vec<u8>, from: DeviceSize, to: (u32, u32)) -> Result<Vec<u8>, PrepError> {
    use fast_image_resize::{PixelType, Resizer, images::Image};
    let src = Image::from_vec_u8(from.w, from.h, rgb, PixelType::U8x3)
        .map_err(|_| PrepError::ShortBuffer)?;
    let mut dst = Image::new(to.0, to.1, PixelType::U8x3);
    Resizer::new()
        .resize(&src, &mut dst, None)
        .map_err(|_| PrepError::Encode)?;
    Ok(dst.into_vec())
}

#[cfg(feature = "pixels")]
fn encode(
    rgb: &[u8],
    (w, h): (u32, u32),
    enc: Encoding,
) -> Result<(MediaType, Vec<u8>), PrepError> {
    use image::{ExtendedColorType, ImageEncoder, codecs};
    let mut out = Vec::new();
    let color = ExtendedColorType::Rgb8;
    let (media, result) = match enc {
        Encoding::Png => (
            MediaType::Png,
            codecs::png::PngEncoder::new(&mut out).write_image(rgb, w, h, color),
        ),
        Encoding::Jpeg(quality) => (
            MediaType::Jpeg,
            codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.value())
                .write_image(rgb, w, h, color),
        ),
    };
    result.map_err(|_| PrepError::Encode)?;
    Ok((media, out))
}
