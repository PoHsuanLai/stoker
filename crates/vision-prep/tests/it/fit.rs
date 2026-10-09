//! `fit` against the reference `smart_resize` (fixtures/smart_resize.csv, made by
//! dev/smart-resize-vectors.py from qwen-vl-utils) and tables for the other rules.

use cua_action::{Coord, DeviceSize, ImageSpace, Size};
use proptest::prelude::*;
use vision_prep::{FitError, ImageTokens, PatchFactor, PixelCount, ResizeRule, fit, image_tokens};

fn smart(factor: u32, min: u64, max: u64) -> ResizeRule {
    ResizeRule::SmartResize {
        factor: PatchFactor(factor),
        min_pixels: PixelCount(min),
        max_pixels: PixelCount(max),
    }
}

fn image(w: u32, h: u32) -> Size<ImageSpace> {
    Size::new(Coord(w), Coord(h))
}

fn field(cell: &str) -> u64 {
    cell.parse().unwrap()
}

#[test]
fn fit_matches_reference_smart_resize() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/smart_resize.csv"
    );
    let csv = std::fs::read_to_string(path).unwrap();
    let mut rows = 0;
    for line in csv.lines().skip(1) {
        let c: Vec<&str> = line.split(',').collect();
        let rule = smart(field(c[2]) as u32, field(c[3]), field(c[4]));
        let src = DeviceSize {
            w: field(c[1]) as u32,
            h: field(c[0]) as u32,
        };
        let expected = match c[5] {
            "" => Err(FitError::AspectTooExtreme),
            h => Ok(image(field(c[6]) as u32, field(h) as u32)),
        };
        assert_eq!(fit(src, &rule), expected, "row {line}");
        rows += 1;
    }
    assert!(rows >= 100, "the fixture has {rows} rows");
}

#[test]
fn zero_area_and_degenerate_rules_are_refused() {
    let rule = smart(32, 65_536, 16_777_216);
    for src in [DeviceSize { w: 0, h: 10 }, DeviceSize { w: 10, h: 0 }] {
        assert_eq!(fit(src, &rule), Err(FitError::ZeroArea));
        assert_eq!(fit(src, &ResizeRule::Identity), Err(FitError::ZeroArea));
    }
    let src = DeviceSize { w: 100, h: 100 };
    assert_eq!(fit(src, &smart(0, 1, 10)), Err(FitError::ZeroArea));
    assert_eq!(fit(src, &smart(32, 0, 0)), Err(FitError::ZeroArea));
    // Floored to nothing: one patch does not fit under a one-pixel budget.
    assert_eq!(fit(src, &smart(32, 1, 1)), Err(FitError::BelowMinimum));
}

#[test]
fn long_edge_table() {
    let rule = ResizeRule::LongEdge {
        max_edge: Coord(1568),
        max_pixels: PixelCount(1_150_000),
    };
    let cases = [
        ((1366, 768), (1366, 768)), // under both caps: unchanged
        ((1568, 882), (1568, 733)), // area cap binds: 1_150_000 pixels
        ((3136, 100), (1568, 50)),  // edge cap binds
        ((1, 1), (1, 1)),
    ];
    for ((w, h), (ow, oh)) in cases {
        let got = fit(DeviceSize { w, h }, &rule).unwrap();
        // The area cap case is computed, not guessed: only the caps are asserted for it.
        if (w, h) == (1568, 882) {
            assert!(u64::from(got.w.0) * u64::from(got.h.0) <= 1_150_000);
            assert!((1420..=1440).contains(&got.w.0));
        } else {
            assert_eq!(got, image(ow, oh), "{w}x{h}");
        }
    }
}

#[test]
fn identity_keeps_the_frame() {
    let got = fit(DeviceSize { w: 1234, h: 567 }, &ResizeRule::Identity);
    assert_eq!(got, Ok(image(1234, 567)));
}

#[test]
fn image_tokens_table() {
    let qwen = smart(28, 3136, 1_003_520);
    assert_eq!(image_tokens(&qwen, image(1260, 784)), ImageTokens(45 * 28));
    assert_eq!(image_tokens(&qwen, image(28, 28)), ImageTokens(1));
    let holo = smart(32, 65_536, 16_777_216);
    assert_eq!(image_tokens(&holo, image(1376, 768)), ImageTokens(43 * 24));
    // Not a multiple of the factor: rounded up, never zero for a real image.
    assert_eq!(image_tokens(&holo, image(33, 1)), ImageTokens(2));
    let long = ResizeRule::LongEdge {
        max_edge: Coord(1568),
        max_pixels: PixelCount(1_150_000),
    };
    assert_eq!(image_tokens(&long, image(1500, 750)), ImageTokens(1500));
    assert_eq!(
        image_tokens(&ResizeRule::Identity, image(750, 1)),
        ImageTokens(1)
    );
    assert_eq!(image_tokens(&holo, image(0, 0)), ImageTokens(0));
}

proptest! {
    #[test]
    fn smart_resize_output_is_a_multiple_within_the_pixel_band(
        w in 1u32..8000, h in 1u32..8000, factor in prop::sample::select(vec![28u32, 32]),
    ) {
        let rule = smart(factor, 65_536, 16_777_216);
        if let Ok(size) = fit(DeviceSize { w, h }, &rule) {
            prop_assert_eq!(size.w.0 % factor, 0);
            prop_assert_eq!(size.h.0 % factor, 0);
            prop_assert!(u64::from(size.w.0) * u64::from(size.h.0) <= 16_777_216);
        }
    }

    #[test]
    fn long_edge_never_exceeds_its_caps(w in 1u32..20_000, h in 1u32..20_000) {
        let rule = ResizeRule::LongEdge { max_edge: Coord(1568), max_pixels: PixelCount(1_150_000) };
        if let Ok(size) = fit(DeviceSize { w, h }, &rule) {
            prop_assert!(size.w.0.max(size.h.0) <= 1568);
            prop_assert!(u64::from(size.w.0) * u64::from(size.h.0) <= 1_150_000);
        }
    }
}
