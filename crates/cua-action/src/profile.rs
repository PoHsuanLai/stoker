//! How one model's tool calls differ from the common computer-use tool, as data.
//!
//! There is no standard for a screen model's actions, but one convention is widely copied:
//! Anthropic's computer-use tool as Qwen wrote it, one `computer_use` function with an `action`
//! verb and a `coordinate: [x, y]` (`ToolDialect::QwenComputerUse`). Most open screen models are
//! Qwen fine-tunes that speak a variant of it: a verb renamed, a drag from the cursor instead of
//! from a start point, a scroll whose axis is an argument. An [`ActionProfile`] states those
//! differences in the model's catalogue entry, next to the dialect, so that a new model of the
//! same family needs a line of data and no code. The default profile states none.

use serde::{Deserialize, Serialize};

use crate::{AnswerShape, CallFormat, ResolutionWording};

/// Where a drag starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragStyle {
    /// `start_coordinate` is the press and `coordinate` the release (the common convention).
    #[default]
    FromStart,
    /// The press is at the cursor, `coordinate` is the release.
    FromCursor,
}

/// How a scroll names its axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollAxis {
    /// `scroll` is vertical and `hscroll` horizontal (the common convention).
    #[default]
    ByVerb,
    /// Only `scroll` exists, and its `direction` is `vertical` (the default) or `horizontal`.
    ByDirection,
}

/// A verb some models write for a verb of the common convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerbAlias {
    /// `drag` for `left_click_drag`.
    Drag,
    /// `finished` for `terminate`.
    Finished,
    /// `hotkey` for `key`.
    Hotkey,
    /// `press` for `key`.
    Press,
    /// `call_user` for asking the person (`text` is the question).
    CallUser,
}

impl VerbAlias {
    const ALL: [VerbAlias; 5] = [
        VerbAlias::Drag,
        VerbAlias::Finished,
        VerbAlias::Hotkey,
        VerbAlias::Press,
        VerbAlias::CallUser,
    ];

    /// The verb the model writes; the same as the alias's slug.
    pub const fn model_verb(self) -> &'static str {
        match self {
            VerbAlias::Drag => "drag",
            VerbAlias::Finished => "finished",
            VerbAlias::Hotkey => "hotkey",
            VerbAlias::Press => "press",
            VerbAlias::CallUser => "call_user",
        }
    }

    /// The verb of the common convention it stands for. `ask` is no verb of that convention: a
    /// model that writes `call_user` is shown it as an extra verb, and it reads as an ask.
    pub const fn canonical(self) -> &'static str {
        match self {
            VerbAlias::Drag => "left_click_drag",
            VerbAlias::Finished => "terminate",
            VerbAlias::Hotkey | VerbAlias::Press => "key",
            VerbAlias::CallUser => "ask",
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// A set of [`VerbAlias`], small enough to be `Copy`; written as a list in a catalogue file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "Vec<VerbAlias>", into = "Vec<VerbAlias>")]
pub struct VerbAliases(u8);

impl From<Vec<VerbAlias>> for VerbAliases {
    fn from(aliases: Vec<VerbAlias>) -> Self {
        VerbAliases(aliases.into_iter().fold(0, |set, alias| set | alias.bit()))
    }
}

impl From<VerbAliases> for Vec<VerbAlias> {
    fn from(set: VerbAliases) -> Self {
        set.iter().collect()
    }
}

impl VerbAliases {
    /// The aliases in the set, in a fixed order.
    pub fn iter(self) -> impl Iterator<Item = VerbAlias> {
        VerbAlias::ALL
            .into_iter()
            .filter(move |alias| self.0 & alias.bit() != 0)
    }

    /// The alias a model's verb is, if the set has it.
    pub fn resolve(self, verb: &str) -> Option<VerbAlias> {
        self.iter().find(|alias| alias.model_verb() == verb)
    }

    /// The aliases standing for one verb of the common convention.
    pub fn of<'a>(self, canonical: &'a str) -> impl Iterator<Item = VerbAlias> + 'a {
        self.iter()
            .filter(move |alias| alias.canonical() == canonical)
    }
}

/// The differences between one model's tool calls and the common convention. Optional in a
/// catalogue entry; the default is the common convention itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionProfile {
    /// The verbs the model writes in place of the common ones.
    #[serde(default)]
    pub aliases: VerbAliases,
    #[serde(default)]
    pub drag: DragStyle,
    #[serde(default)]
    pub scroll: ScrollAxis,
    /// Who reads the calls out of the reply text, and in which format.
    #[serde(default, skip_serializing_if = "is_default")]
    pub calls: CallFormat,
    /// The shape of a whole reply the model was trained to write.
    #[serde(default, skip_serializing_if = "is_default")]
    pub answer: AnswerShape,
    /// How the grid's size is worded when the model is told the screen's resolution.
    #[serde(default, skip_serializing_if = "is_default")]
    pub resolution: ResolutionWording,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

impl ActionProfile {
    /// Whether this is the common convention, stating no difference.
    pub fn is_plain(&self) -> bool {
        *self == ActionProfile::default()
    }

    /// Whether the verbs, the drag and the scroll are the common convention: the tool schema a
    /// model is shown is then the shipped file itself. The other options of the profile (how the
    /// calls are read, the shape of the answer) leave the schema alone.
    pub fn verbs_plain(&self) -> bool {
        self.aliases == VerbAliases::default()
            && self.drag == DragStyle::default()
            && self.scroll == ScrollAxis::default()
    }
}
