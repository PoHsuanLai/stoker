//! Points, sizes and the units around them.

use core::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::CoordSpace;

/// One coordinate in a space's own unit. Whole numbers: a sub-pixel click means nothing, and
/// data stays `Eq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Coord(pub u32);

/// A point in space `S`.
///
/// ```compile_fail
/// use cua_action::{Coord, ImageSpace, Point, WindowSpace};
/// let image: Point<ImageSpace> = Point::new(Coord(1), Coord(2));
/// let window: Point<WindowSpace> = image;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Point<S: CoordSpace> {
    pub x: Coord,
    pub y: Coord,
    #[serde(skip)]
    space: PhantomData<S>,
}

impl<S: CoordSpace> Point<S> {
    pub fn new(x: Coord, y: Coord) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }
}

/// A width and height in space `S`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Size<S: CoordSpace> {
    pub w: Coord,
    pub h: Coord,
    #[serde(skip)]
    space: PhantomData<S>,
}

impl<S: CoordSpace> Size<S> {
    pub fn new(w: Coord, h: Coord) -> Self {
        Self {
            w,
            h,
            space: PhantomData,
        }
    }
}

/// A rectangle in space `S`: top-left corner and extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Rect<S: CoordSpace> {
    pub origin: Point<S>,
    pub size: Size<S>,
}

/// A distance along one axis in space `S` (a scroll distance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(bound = "", transparent)]
pub struct Length<S: CoordSpace> {
    coord: Coord,
    space: PhantomData<S>,
}

impl<S: CoordSpace> Length<S> {
    pub fn new(coord: Coord) -> Self {
        Self {
            coord,
            space: PhantomData,
        }
    }

    pub fn coord(&self) -> Coord {
        self.coord
    }
}

/// Device pixels per logical pixel, in 120ths (the `wp-fractional-scale` unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scale120(pub u32);

/// The top of a model's normalised grid: 999 (Gemini) or 1000 (the Qwen3-VL family). A parameter
/// of the dialect profile, never a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GridMax(pub u16);

/// A buffer's size in device pixels (logical size times scale over 120).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceSize {
    pub w: u32,
    pub h: u32,
}

/// The `wl_shm` pixel formats a capturer hands over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Xrgb8888,
    Argb8888,
    Xbgr8888,
    Abgr8888,
}
