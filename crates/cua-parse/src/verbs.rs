//! The verbs a tool dialect writes, as data: how its calls differ from the common convention
//! (`QwenComputerUse`'s `computer_use` function with an `action` argument). A new dialect adds
//! a table here, not an arm in the parser.

use cua_action::ToolDialect;

/// What one tool dialect adds to the common convention.
pub(crate) struct Verbs {
    /// Whether a call may be named by its verb instead of `computer_use` with an `action`.
    pub function_is_verb: bool,
    /// Verbs the dialect writes under another name: (written, convention verb).
    pub synonyms: &'static [(&'static str, &'static str)],
    /// Verbs only the dialect has.
    pub extra: &'static [&'static str],
    /// Whether `scroll` may give a `direction` and no pixel amount.
    pub scroll_by_direction: bool,
}

const QWEN: Verbs = Verbs {
    function_is_verb: false,
    synonyms: &[],
    extra: &[],
    scroll_by_direction: false,
};

const HOLO: Verbs = Verbs {
    function_is_verb: true,
    synonyms: &[
        ("click", "left_click"),
        ("move", "mouse_move"),
        ("drag", "left_click_drag"),
        ("finish", "terminate"),
    ],
    extra: &["ask", "observe"],
    scroll_by_direction: true,
};

impl Verbs {
    /// The table of `dialect`.
    pub(crate) fn of(dialect: ToolDialect) -> &'static Verbs {
        match dialect {
            ToolDialect::QwenComputerUse => &QWEN,
            ToolDialect::Holo31 => &HOLO,
        }
    }

    /// The convention's name for `verb`: itself, unless the dialect writes it another way.
    pub(crate) fn canonical<'a>(&self, verb: &'a str) -> &'a str {
        self.synonyms
            .iter()
            .find(|(written, _)| *written == verb)
            .map_or(verb, |(_, convention)| convention)
    }

    /// Whether `verb` is one only this dialect has.
    pub(crate) fn has(&self, verb: &str) -> bool {
        self.extra.contains(&verb)
    }
}
