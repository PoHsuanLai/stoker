use cua_action::{Coord, GridMax, ImageSpace, ModelSpace, Size, WindowSpace};
use vision_prep::{
    Encoding, FitError, FrameMap, ImageTokens, JpegQuality, MapError, PatchFactor, PixelCount,
    ResizeRule,
};

fn round_trip<T>(value: &T, json: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + core::fmt::Debug,
{
    assert_eq!(serde_json::to_string(value).unwrap(), json);
    assert_eq!(&serde_json::from_str::<T>(json).unwrap(), value);
}

#[test]
fn resize_rules_round_trip_with_pinned_json() {
    round_trip(
        &ResizeRule::SmartResize {
            factor: PatchFactor(32),
            min_pixels: PixelCount(65_536),
            max_pixels: PixelCount(16_777_216),
        },
        r#"{"kind":"smart_resize","v":{"factor":32,"min_pixels":65536,"max_pixels":16777216}}"#,
    );
    round_trip(
        &ResizeRule::LongEdge {
            max_edge: Coord(1568),
            max_pixels: PixelCount(1_150_000),
        },
        r#"{"kind":"long_edge","v":{"max_edge":1568,"max_pixels":1150000}}"#,
    );
    round_trip(&ResizeRule::Identity, r#"{"kind":"identity"}"#);
}

#[test]
fn errors_and_encodings_round_trip() {
    round_trip(&FitError::AspectTooExtreme, r#""aspect_too_extreme""#);
    round_trip(
        &MapError::OutOfFrame {
            x: Coord(5),
            y: Coord(6),
        },
        r#"{"kind":"out_of_frame","v":{"x":5,"y":6}}"#,
    );
    round_trip(&MapError::GridMismatch, r#"{"kind":"grid_mismatch"}"#);
    round_trip(&Encoding::Png, r#"{"kind":"png"}"#);
    round_trip(
        &Encoding::Jpeg(JpegQuality::new(85).unwrap()),
        r#"{"kind":"jpeg","v":85}"#,
    );
    round_trip(&ImageTokens(1024), "1024");
}

#[test]
fn jpeg_quality_refuses_zero_and_over_100() {
    assert!(JpegQuality::new(0).is_err());
    assert!(JpegQuality::new(101).is_err());
    assert!(serde_json::from_str::<JpegQuality>("0").is_err());
}

#[test]
fn frame_map_round_trips() {
    let map = FrameMap {
        window: Size::<WindowSpace>::new(Coord(1366), Coord(768)),
        image: Size::<ImageSpace>::new(Coord(1376), Coord(768)),
        space: ModelSpace::Grid(GridMax(1000)),
    };
    round_trip(
        &map,
        r#"{"window":{"w":1366,"h":768},"image":{"w":1376,"h":768},"space":{"kind":"grid","v":1000}}"#,
    );
}
