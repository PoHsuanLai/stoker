//! The frame map: tables, the refusal rule, and the round trip.

use cua_action::{
    Coord, GridMax, GridSpace, ImageSpace, Length, ModelSpace, Point, Scale120, Size, WindowSpace,
};
use proptest::prelude::*;
use vision_prep::{FitError, FrameMap, MapError, PatchFactor, PixelCount, ResizeRule};

fn size<S: cua_action::CoordSpace>(w: u32, h: u32) -> Size<S> {
    Size::new(Coord(w), Coord(h))
}

fn map(window: (u32, u32), image: (u32, u32), space: ModelSpace) -> FrameMap {
    FrameMap {
        window: size::<WindowSpace>(window.0, window.1),
        image: size::<ImageSpace>(image.0, image.1),
        space,
    }
}

fn img(x: u32, y: u32) -> Point<ImageSpace> {
    Point::new(Coord(x), Coord(y))
}

fn grid(x: u32, y: u32) -> Point<GridSpace> {
    Point::new(Coord(x), Coord(y))
}

fn win(x: u32, y: u32) -> Point<WindowSpace> {
    Point::new(Coord(x), Coord(y))
}

#[test]
fn new_fits_the_device_size() {
    let rule = ResizeRule::SmartResize {
        factor: PatchFactor(32),
        min_pixels: PixelCount(65_536),
        max_pixels: PixelCount(16_777_216),
    };
    let m = FrameMap::new(size(1366, 768), Scale120(120), &rule, ModelSpace::Image).unwrap();
    assert_eq!(m.image, size(1376, 768));
    assert_eq!(m.window, size(1366, 768));
    // 1.5x: 2049 x 1152 device pixels.
    let m = FrameMap::new(size(1366, 768), Scale120(180), &rule, ModelSpace::Image).unwrap();
    assert_eq!(m.image, size(2048, 1152));
    // Fractional scale rounds half up: 1366 * 150 / 120 = 1707.5 -> 1708.
    let id = ResizeRule::Identity;
    let m = FrameMap::new(size(1366, 768), Scale120(150), &id, ModelSpace::Image).unwrap();
    assert_eq!(m.image, size(1708, 960));
}

#[test]
fn new_refuses_an_empty_window() {
    let id = ResizeRule::Identity;
    let zero = FrameMap::new(size(0, 10), Scale120(120), &id, ModelSpace::Image);
    assert_eq!(zero, Err(FitError::ZeroArea));
    let no_scale = FrameMap::new(size(10, 10), Scale120(0), &id, ModelSpace::Image);
    assert_eq!(no_scale, Err(FitError::ZeroArea));
}

#[test]
fn image_points_map_with_round_half_up() {
    let m = map((1000, 500), (400, 200), ModelSpace::Image);
    assert_eq!(m.image_to_window(img(0, 0)), Ok(win(0, 0)));
    assert_eq!(m.image_to_window(img(1, 1)), Ok(win(3, 3))); // 2.5 rounds up
    assert_eq!(m.image_to_window(img(399, 199)), Ok(win(998, 498))); // 997.5 -> 998
    let down = map((100, 100), (300, 300), ModelSpace::Image);
    assert_eq!(down.image_to_window(img(1, 2)), Ok(win(0, 1))); // 0.33 -> 0, 0.67 -> 1
}

#[test]
fn outside_image_is_refused() {
    let m = map((1366, 768), (1376, 768), ModelSpace::Image);
    assert_eq!(
        m.image_to_window(img(1376, 10)),
        Err(MapError::OutOfFrame {
            x: Coord(1376),
            y: Coord(10)
        })
    );
    assert_eq!(
        m.image_to_window(img(10, 768)),
        Err(MapError::OutOfFrame {
            x: Coord(10),
            y: Coord(768)
        })
    );
    // Inside the image but rounding onto the window edge (299 * 100 / 300 = 99.67): refused.
    let tight = map((100, 100), (300, 300), ModelSpace::Image);
    assert_eq!(
        tight.image_to_window(img(299, 0)),
        Err(MapError::OutOfFrame {
            x: Coord(299),
            y: Coord(0)
        })
    );
    assert_eq!(
        m.image_to_window(img(u32::MAX, u32::MAX)),
        Err(MapError::OutOfFrame {
            x: Coord(u32::MAX),
            y: Coord(u32::MAX)
        })
    );
}

#[test]
fn grid_999_and_1000_differ() {
    let p = grid(500, 250);
    let a = map((1000, 1000), (1000, 1000), ModelSpace::Grid(GridMax(999)));
    let b = map((1000, 1000), (1000, 1000), ModelSpace::Grid(GridMax(1000)));
    assert_eq!(a.grid_to_window(p), Ok(win(501, 250))); // 500.5, 250.25
    assert_eq!(b.grid_to_window(p), Ok(win(500, 250)));
    // The divisor is the grid max, not the image size.
    let wide = map((2000, 1000), (700, 700), ModelSpace::Grid(GridMax(1000)));
    assert_eq!(wide.grid_to_window(grid(500, 500)), Ok(win(1000, 500)));
}

#[test]
fn the_grid_edge_is_outside_the_frame() {
    let m = map((1000, 1000), (1000, 1000), ModelSpace::Grid(GridMax(1000)));
    assert_eq!(m.grid_to_window(grid(999, 999)), Ok(win(999, 999)));
    assert_eq!(
        m.grid_to_window(grid(1000, 0)),
        Err(MapError::OutOfFrame {
            x: Coord(1000),
            y: Coord(0)
        })
    );
    assert!(m.grid_to_window(grid(5000, 5000)).is_err());
}

#[test]
fn a_point_in_the_wrong_space_is_a_mismatch() {
    let image_map = map((100, 100), (100, 100), ModelSpace::Image);
    let grid_map = map((100, 100), (100, 100), ModelSpace::Grid(GridMax(1000)));
    assert_eq!(
        image_map.grid_to_window(grid(1, 1)),
        Err(MapError::GridMismatch)
    );
    assert_eq!(
        grid_map.image_to_window(img(1, 1)),
        Err(MapError::GridMismatch)
    );
    assert_eq!(
        grid_map.length_to_window(Length::<ImageSpace>::new(Coord(5))),
        Err(MapError::GridMismatch)
    );
    assert_eq!(
        image_map.length_to_window(Length::<GridSpace>::new(Coord(5))),
        Err(MapError::GridMismatch)
    );
}

#[test]
fn lengths_scale_by_the_width_ratio() {
    let m = map((1000, 500), (400, 200), ModelSpace::Image);
    let got = m
        .length_to_window(Length::<ImageSpace>::new(Coord(100)))
        .unwrap();
    assert_eq!(got.coord(), Coord(250));
    let g = map((1000, 500), (400, 200), ModelSpace::Grid(GridMax(1000)));
    let got = g
        .length_to_window(Length::<GridSpace>::new(Coord(100)))
        .unwrap();
    assert_eq!(got.coord(), Coord(100));
    let same = m
        .length_to_window(Length::<WindowSpace>::new(Coord(7)))
        .unwrap();
    assert_eq!(same.coord(), Coord(7));
}

#[test]
fn a_map_with_an_empty_image_refuses_instead_of_dividing() {
    let m = map((100, 100), (0, 0), ModelSpace::Image);
    assert!(m.image_to_window(img(0, 0)).is_err());
    assert!(
        m.length_to_window(Length::<ImageSpace>::new(Coord(1)))
            .is_err()
    );
    assert_eq!(m.window_to_image(win(5, 5)), img(0, 0));
}

#[test]
fn window_to_image_never_passes_the_last_pixel() {
    let m = map((100, 100), (101, 101), ModelSpace::Image);
    assert_eq!(m.window_to_image(win(99, 99)), img(100, 100));
    assert_eq!(m.window_to_image(win(5000, 5000)), img(100, 100));
}

proptest! {
    #[test]
    fn map_round_trips_within_one_px(
        ww in 1u32..4000, wh in 1u32..4000, grow_w in 0u32..4000, grow_h in 0u32..4000,
        fx in 0.0f64..1.0, fy in 0.0f64..1.0,
    ) {
        // The image is at least as large as the window: a coarser image cannot give a pixel back.
        let m = map((ww, wh), (ww + grow_w, wh + grow_h), ModelSpace::Image);
        let (x, y) = ((f64::from(ww) * fx) as u32, (f64::from(wh) * fy) as u32);
        let back = m.image_to_window(m.window_to_image(win(x, y))).unwrap();
        prop_assert!(back.x.0.abs_diff(x) <= 1 && back.y.0.abs_diff(y) <= 1,
            "({x},{y}) came back as ({},{})", back.x.0, back.y.0);
    }

    #[test]
    fn image_points_inside_map_inside_or_refuse(
        ww in 1u32..4000, wh in 1u32..4000, iw in 1u32..4000, ih in 1u32..4000,
        x in 0u32..8000, y in 0u32..8000,
    ) {
        let m = map((ww, wh), (iw, ih), ModelSpace::Image);
        match m.image_to_window(img(x, y)) {
            Ok(p) => prop_assert!(p.x.0 < ww && p.y.0 < wh && x < iw + 1 && y < ih + 1),
            Err(MapError::OutOfFrame { x: ex, y: ey }) => prop_assert_eq!((ex, ey), (Coord(x), Coord(y))),
            Err(MapError::GridMismatch) => prop_assert!(false),
        }
    }
}
