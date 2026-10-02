//! The map between the three coordinate spaces of one step.

use cua_action::{
    Coord, GridSpace, ImageSpace, Length, ModelSpace, Point, Scale120, Size, WindowSpace,
};
use serde::{Deserialize, Serialize};

use crate::{FitError, ResizeRule};

/// A point or length that does not map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum MapError {
    /// Outside the image: refused, never clamped.
    #[error("({x:?}, {y:?}) is outside the frame")]
    OutOfFrame { x: Coord, y: Coord },
    /// A grid point was given to a map built for an image-space dialect, or the reverse.
    #[error("the point's space does not match the map's model space")]
    GridMismatch,
}

/// One step's frame geometry: the window the model acts on, the image it was shown, and where
/// the dialect's points live. `x_window = round_half_up(x * window.w / image.w)` in `u64`, and
/// the result must be below `window.w`; the grid rule is the same with `max` as the divisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameMap {
    pub window: Size<WindowSpace>,
    pub image: Size<ImageSpace>,
    pub space: ModelSpace,
}

impl FrameMap {
    /// Fits the window's device-pixel size (logical times `scale` over 120) to `rule`.
    pub fn new(
        window: Size<WindowSpace>,
        scale: Scale120,
        rule: &ResizeRule,
        space: ModelSpace,
    ) -> Result<FrameMap, FitError> {
        let _ = (window, scale, rule, space);
        todo!("FrameMap::new: device size from scale, then fit")
    }

    pub fn image_to_window(&self, p: Point<ImageSpace>) -> Result<Point<WindowSpace>, MapError> {
        let _ = p;
        todo!("image_to_window: round half up in u64, refuse outside")
    }

    pub fn grid_to_window(&self, p: Point<GridSpace>) -> Result<Point<WindowSpace>, MapError> {
        let _ = p;
        todo!("grid_to_window: divisor is the grid max")
    }

    /// For "cursor at" hints and replay.
    pub fn window_to_image(&self, p: Point<WindowSpace>) -> Point<ImageSpace> {
        let _ = p;
        todo!("window_to_image")
    }

    pub fn length_to_window<S: cua_action::CoordSpace>(
        &self,
        l: Length<S>,
    ) -> Result<Length<WindowSpace>, MapError> {
        let _ = l;
        todo!("length_to_window")
    }
}
