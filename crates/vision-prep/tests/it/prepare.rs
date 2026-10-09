//! `prepare` resizes once and encodes once; compiled only with the `pixels` feature.
#![cfg(feature = "pixels")]

use cua_action::{Coord, DeviceSize, ImageSpace, ModelSpace, PixelFormat, Size, WindowSpace};
use vision_prep::{Encoding, FrameMap, JpegQuality, MediaType, PrepError, RawFrame, prepare};

fn map(window: (u32, u32), image: (u32, u32)) -> FrameMap {
    FrameMap {
        window: Size::<WindowSpace>::new(Coord(window.0), Coord(window.1)),
        image: Size::<ImageSpace>::new(Coord(image.0), Coord(image.1)),
        space: ModelSpace::Image,
    }
}

/// A frame of one flat colour, bytes in the order the format stores them.
fn flat(w: u32, h: u32, stride: u32, px: [u8; 4]) -> Vec<u8> {
    let mut buf = vec![0u8; (stride * h) as usize];
    for row in buf.chunks_mut(stride as usize) {
        for pixel in row[..(w * 4) as usize].as_chunks_mut::<4>().0 {
            *pixel = px;
        }
    }
    buf
}

fn frame(buf: &[u8], w: u32, h: u32, stride: u32, format: PixelFormat) -> RawFrame<'_> {
    RawFrame {
        pixels: buf,
        size: DeviceSize { w, h },
        stride,
        format,
    }
}

fn decode(bytes: &[u8]) -> image::RgbImage {
    image::load_from_memory(bytes).unwrap().to_rgb8()
}

#[test]
fn png_at_the_same_size_is_lossless_and_reorders_channels() {
    let cases = [
        (PixelFormat::Xrgb8888, [30, 20, 10, 0]),
        (PixelFormat::Argb8888, [30, 20, 10, 255]),
        (PixelFormat::Xbgr8888, [10, 20, 30, 0]),
        (PixelFormat::Abgr8888, [10, 20, 30, 255]),
    ];
    for (format, px) in cases {
        let buf = flat(8, 4, 32, px);
        let out = prepare(
            frame(&buf, 8, 4, 32, format),
            &map((8, 4), (8, 4)),
            Encoding::Png,
        )
        .unwrap();
        assert_eq!(out.media, MediaType::Png);
        assert_eq!(out.size, Size::<ImageSpace>::new(Coord(8), Coord(4)));
        let img = decode(&out.bytes);
        assert!(img.pixels().all(|p| p.0 == [10, 20, 30]), "{format:?}");
    }
}

#[test]
fn stride_padding_is_skipped() {
    let buf = flat(5, 3, 64, [1, 2, 3, 0]);
    let out = prepare(
        frame(&buf, 5, 3, 64, PixelFormat::Xbgr8888),
        &map((5, 3), (5, 3)),
        Encoding::Png,
    )
    .unwrap();
    assert_eq!(decode(&out.bytes).dimensions(), (5, 3));
}

#[test]
fn resizes_once_to_the_map_image_size() {
    let buf = flat(200, 100, 800, [0, 255, 0, 0]);
    let m = map((200, 100), (112, 56));
    let out = prepare(
        frame(&buf, 200, 100, 800, PixelFormat::Xrgb8888),
        &m,
        Encoding::Png,
    )
    .unwrap();
    assert_eq!(out.size, m.image);
    let img = decode(&out.bytes);
    assert_eq!(img.dimensions(), (112, 56));
    assert!(
        img.pixels()
            .all(|p| p.0[1] >= 253 && p.0[0] <= 2 && p.0[2] <= 2)
    );
}

#[test]
fn jpeg_encodes_at_the_requested_quality() {
    let buf = flat(64, 64, 256, [128, 128, 128, 0]);
    let m = map((64, 64), (32, 32));
    let q = |n| Encoding::Jpeg(JpegQuality::new(n).unwrap());
    let low = prepare(frame(&buf, 64, 64, 256, PixelFormat::Xrgb8888), &m, q(10)).unwrap();
    let high = prepare(frame(&buf, 64, 64, 256, PixelFormat::Xrgb8888), &m, q(95)).unwrap();
    assert_eq!(low.media, MediaType::Jpeg);
    assert_eq!(&low.bytes[..2], &[0xFF, 0xD8]);
    assert_eq!(decode(&high.bytes).dimensions(), (32, 32));
    assert!(low.bytes.len() <= high.bytes.len());
}

#[test]
fn short_buffers_and_bad_geometry_are_refused() {
    let buf = flat(8, 4, 32, [0; 4]);
    let m = map((8, 4), (8, 4));
    let short = &buf[..buf.len() - 1];
    assert_eq!(
        prepare(
            frame(short, 8, 4, 32, PixelFormat::Xrgb8888),
            &m,
            Encoding::Png
        )
        .unwrap_err(),
        PrepError::ShortBuffer
    );
    // A stride narrower than a row.
    assert_eq!(
        prepare(
            frame(&buf, 8, 4, 16, PixelFormat::Xrgb8888),
            &m,
            Encoding::Png
        )
        .unwrap_err(),
        PrepError::ShortBuffer
    );
    // A frame whose shape is not the window's.
    let wide = flat(16, 4, 64, [0; 4]);
    assert_eq!(
        prepare(
            frame(&wide, 16, 4, 64, PixelFormat::Xrgb8888),
            &m,
            Encoding::Png
        )
        .unwrap_err(),
        PrepError::SizeMismatch
    );
    assert_eq!(
        prepare(
            frame(&[], 0, 0, 0, PixelFormat::Xrgb8888),
            &m,
            Encoding::Png
        )
        .unwrap_err(),
        PrepError::SizeMismatch
    );
}

#[test]
fn a_scaled_frame_is_not_a_mismatch() {
    // 1366 x 768 at 1.25: 1708 x 960 device pixels (1707.5 rounds up).
    let buf = flat(1708, 960, 1708 * 4, [9, 9, 9, 0]);
    let m = map((1366, 768), (1376, 768));
    let out = prepare(
        frame(&buf, 1708, 960, 1708 * 4, PixelFormat::Xrgb8888),
        &m,
        Encoding::Png,
    )
    .unwrap();
    assert_eq!(decode(&out.bytes).dimensions(), (1376, 768));
}
