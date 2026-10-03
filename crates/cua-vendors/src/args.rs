//! Reading the arguments of one vendor call: a JSON object, and the few shapes its fields take.
//! Every reader answers a `DropReason`, never a panic.

use cua_action::{Coord, CoordSpace, Point};
use cua_parse::DropReason;
use serde_json::{Map, Value};

pub(crate) type Args = Map<String, Value>;
pub(crate) type Verdict<T> = Result<T, DropReason>;

/// The call's input as an object, or `None` when it is anything else.
pub(crate) fn object(input: &str) -> Option<Args> {
    match serde_json::from_str::<Value>(input) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// A whole number that fits `u32`: a JSON integer, or a float with nothing after the point.
pub(crate) fn whole(value: &Value) -> Verdict<u32> {
    let n = value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= 0.0 && *f <= f64::from(u32::MAX))
                .map(|f| f as u64)
        })
        .ok_or(DropReason::BadNumber)?;
    u32::try_from(n).map_err(|_| DropReason::BadNumber)
}

/// A signed whole number, for scroll distances.
pub(crate) fn signed(value: &Value) -> Verdict<i64> {
    value
        .as_i64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|f| f.fract() == 0.0 && f.abs() <= 1e9)
                .map(|f| f as i64)
        })
        .ok_or(DropReason::BadNumber)
}

pub(crate) fn field<'a>(args: &'a Args, key: &str) -> Verdict<&'a Value> {
    args.get(key).ok_or(DropReason::MissingArgument)
}

pub(crate) fn string<'a>(args: &'a Args, key: &str) -> Verdict<&'a str> {
    field(args, key)?.as_str().ok_or(DropReason::BadArgument)
}

pub(crate) fn number(args: &Args, key: &str) -> Verdict<u32> {
    whole(field(args, key)?)
}

/// `key: [x, y]`.
pub(crate) fn pair(args: &Args, key: &str) -> Verdict<(u32, u32)> {
    match field(args, key)?.as_array().map(Vec::as_slice) {
        Some([x, y]) => Ok((whole(x)?, whole(y)?)),
        Some(_) | None => Err(DropReason::BadArgument),
    }
}

/// `x` and `y` fields, each named with a prefix: `start_x`, `start_y`.
pub(crate) fn xy(args: &Args, x: &str, y: &str) -> Verdict<(u32, u32)> {
    Ok((number(args, x)?, number(args, y)?))
}

/// A point of the vendor's space; a coordinate past `top` (when the space has one) is refused,
/// never clamped.
pub(crate) fn point<S: CoordSpace>(at: (u32, u32), top: Option<u32>) -> Verdict<Point<S>> {
    match top {
        Some(top) if at.0 > top || at.1 > top => Err(DropReason::OutOfFrame),
        _ => Ok(Point::new(Coord(at.0), Coord(at.1))),
    }
}
