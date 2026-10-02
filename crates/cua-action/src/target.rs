//! What an action points at.

use serde::{Deserialize, Serialize};

use crate::{CoordSpace, Point};

/// An accessibility node, stable within one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

/// Where a pointer action lands: a point, or a node when the dialect can name one and a tree
/// exists. A node passes through every space mapping unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case", bound = "")]
pub enum Target<S: CoordSpace> {
    Point(Point<S>),
    Node(NodeId),
}
