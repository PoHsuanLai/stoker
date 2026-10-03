//! What an action points at.

use serde::{Deserialize, Serialize};

use crate::{CoordSpace, Point};

/// An accessibility node, stable within one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

/// Where a pointer action lands: a point, or a node when the dialect can name one and a tree
/// exists, or the middle of the frame when the model named nowhere (Qwen's `scroll` takes no
/// coordinate). A node and the centre pass through every space mapping unchanged; the executor
/// resolves the centre against the window it acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case", bound = "")]
pub enum Target<S: CoordSpace> {
    Point(Point<S>),
    Node(NodeId),
    /// The centre of the frame the model saw.
    Centre,
}
