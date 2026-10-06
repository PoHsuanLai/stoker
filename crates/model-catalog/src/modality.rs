//! What goes into a model and what comes out of it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// One kind of thing a model takes in or gives out. `Vector` and `Actions` are outputs only: no
/// model is handed an embedding or a list of actions, and the parser refuses them as inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    /// An embedding vector.
    Vector,
    /// Computer-use actions (clicks, keys, scrolls).
    Actions,
}

impl Modality {
    /// Whether a model can be handed this; `Vector` and `Actions` are for output only.
    pub fn is_input(self) -> bool {
        matches!(self, Modality::Text | Modality::Image | Modality::Audio)
    }
}

/// A set of modalities, written as a list in a catalog file: `["text", "image"]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Modalities(pub BTreeSet<Modality>);

impl Modalities {
    pub fn contains(&self, modality: Modality) -> bool {
        self.0.contains(&modality)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether every member of `self` is also in `wider`.
    pub fn is_subset_of(&self, wider: &Modalities) -> bool {
        self.0.is_subset(&wider.0)
    }

    /// The members both sets have.
    pub fn intersection(&self, other: &Modalities) -> Modalities {
        Modalities(self.0.intersection(&other.0).copied().collect())
    }

    /// The first member that cannot be an input, if any.
    pub fn first_output_only(&self) -> Option<Modality> {
        self.0.iter().copied().find(|m| !m.is_input())
    }
}

impl<const N: usize> From<[Modality; N]> for Modalities {
    fn from(list: [Modality; N]) -> Self {
        Modalities(list.into())
    }
}
