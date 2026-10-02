//! The map between the three coordinate spaces of one step.

use core::any::TypeId;

use cua_action::{
    Coord, CoordSpace, DeviceSize, GridSpace, ImageSpace, Length, ModelSpace, Point, Scale120,
    Size, WindowSpace,
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

/// `round_half_up(v * num / den)` in `u64`; `None` when `den` is zero.
fn scaled(v: u32, num: u32, den: u32) -> Option<u64> {
    let den = u64::from(den);
    (den != 0).then(|| (2 * u64::from(v) * u64::from(num) + den) / (2 * den))
}

/// A mapped coordinate that must land inside `0..limit`.
fn within(v: u32, num: u32, den: u32, limit: Coord) -> Option<Coord> {
    scaled(v, num, den)
        .and_then(|mapped| u32::try_from(mapped).ok())
        .filter(|mapped| *mapped < limit.0)
        .map(Coord)
}

impl FrameMap {
    /// Fits the window's device-pixel size (logical times `scale` over 120) to `rule`.
    pub fn new(
        window: Size<WindowSpace>,
        scale: Scale120,
        rule: &ResizeRule,
        space: ModelSpace,
    ) -> Result<FrameMap, FitError> {
        let device = |logical: Coord| {
            scaled(logical.0, scale.0, 120)
                .and_then(|px| u32::try_from(px).ok())
                .ok_or(FitError::ZeroArea)
        };
        let src = DeviceSize {
            w: device(window.w)?,
            h: device(window.h)?,
        };
        let image = crate::fit(src, rule)?;
        Ok(FrameMap {
            window,
            image,
            space,
        })
    }

    /// Maps a point of the image the model was shown. A map built for a grid dialect refuses
    /// it with `GridMismatch`; a point at or past the image edge is `OutOfFrame`.
    pub fn image_to_window(&self, p: Point<ImageSpace>) -> Result<Point<WindowSpace>, MapError> {
        match self.space {
            ModelSpace::Image => self.map_point(p.x, p.y, self.image.w, self.image.h),
            ModelSpace::Grid(_) => Err(MapError::GridMismatch),
        }
    }

    /// Maps a point of the normalised grid: the divisor is the grid's max, not the image size.
    pub fn grid_to_window(&self, p: Point<GridSpace>) -> Result<Point<WindowSpace>, MapError> {
        match self.space {
            ModelSpace::Grid(max) => {
                let max = Coord(u32::from(max.0));
                self.map_point(p.x, p.y, max, max)
            }
            ModelSpace::Image => Err(MapError::GridMismatch),
        }
    }

    fn map_point(
        &self,
        x: Coord,
        y: Coord,
        across: Coord,
        down: Coord,
    ) -> Result<Point<WindowSpace>, MapError> {
        let out_of_frame = MapError::OutOfFrame { x, y };
        let wx = within(x.0, self.window.w.0, across.0, self.window.w).ok_or(out_of_frame)?;
        let wy = within(y.0, self.window.h.0, down.0, self.window.h).ok_or(out_of_frame)?;
        Ok(Point::new(wx, wy))
    }

    /// For "cursor at" hints and replay. A total function: a window point maps to the nearest
    /// image pixel, never past the last one.
    pub fn window_to_image(&self, p: Point<WindowSpace>) -> Point<ImageSpace> {
        let axis = |v: Coord, image: Coord, window: Coord| {
            let mapped = scaled(v.0, image.0, window.0)
                .and_then(|px| u32::try_from(px).ok())
                .unwrap_or(0);
            Coord(mapped.min(image.0.saturating_sub(1)))
        };
        Point::new(
            axis(p.x, self.image.w, self.window.w),
            axis(p.y, self.image.h, self.window.h),
        )
    }

    /// Maps a distance (a scroll) along either axis, by the width ratio: the resize keeps the
    /// aspect ratio, so the two axes agree to within rounding. A length in the wrong model
    /// space is `GridMismatch`; one already in window space passes through.
    pub fn length_to_window<S: CoordSpace>(
        &self,
        l: Length<S>,
    ) -> Result<Length<WindowSpace>, MapError> {
        let id = TypeId::of::<S>();
        let divisor = match (self.space, id) {
            (_, id) if id == TypeId::of::<WindowSpace>() => return Ok(Length::new(l.coord())),
            (ModelSpace::Image, id) if id == TypeId::of::<ImageSpace>() => self.image.w.0,
            (ModelSpace::Grid(max), id) if id == TypeId::of::<GridSpace>() => u32::from(max.0),
            _ => return Err(MapError::GridMismatch),
        };
        let out_of_frame = MapError::OutOfFrame {
            x: l.coord(),
            y: l.coord(),
        };
        scaled(l.coord().0, self.window.w.0, divisor)
            .and_then(|mapped| u32::try_from(mapped).ok())
            .map(|mapped| Length::new(Coord(mapped)))
            .ok_or(out_of_frame)
    }
}
