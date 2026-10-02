//! Bounded text: every string a model can put into an action has a limit, checked where it
//! enters and again on deserialisation, so a value of these types is always within bounds.

use core::fmt;

use serde::{Deserialize, Serialize};

/// Why a string is not a valid bounded text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum TextError {
    #[error("the text is empty")]
    Empty,
    #[error("the text has {got} characters, the limit is {max}")]
    TooLong { max: u32, got: u32 },
    #[error("a control character at character {at}")]
    ControlChar { at: u32 },
}

/// Which control characters a text may hold.
#[derive(Clone, Copy)]
enum Controls {
    None,
    NewlineAndTab,
}

/// Whether an empty text is valid.
#[derive(Clone, Copy)]
enum Emptiness {
    Allowed,
    Refused,
}

fn check(text: &str, max: u32, controls: Controls, empty: Emptiness) -> Result<(), TextError> {
    if text.is_empty() {
        return match empty {
            Emptiness::Allowed => Ok(()),
            Emptiness::Refused => Err(TextError::Empty),
        };
    }
    let mut count: u32 = 0;
    for (at, c) in text.chars().enumerate() {
        count = count.saturating_add(1);
        let allowed =
            !c.is_control() || matches!((controls, c), (Controls::NewlineAndTab, '\n' | '\t'));
        if !allowed {
            return Err(TextError::ControlChar {
                at: u32::try_from(at).unwrap_or(u32::MAX),
            });
        }
    }
    if count > max {
        return Err(TextError::TooLong { max, got: count });
    }
    Ok(())
}

macro_rules! bounded_text {
    ($(#[$doc:meta])* $name:ident, max = $max:expr, $controls:expr, $empty:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// The most characters this text may hold.
            pub const MAX_CHARS: u32 = $max;

            pub fn new(text: impl Into<String>) -> Result<Self, TextError> {
                let text = text.into();
                check(&text, Self::MAX_CHARS, $controls, $empty)?;
                Ok(Self(text))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = TextError;
            fn try_from(text: String) -> Result<Self, TextError> {
                Self::new(text)
            }
        }

        impl From<$name> for String {
            fn from(text: $name) -> String {
                text.0
            }
        }

        // Model and screen text is personal: Debug shows the length, never the content.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "(<{} chars>)"), self.0.chars().count())
            }
        }
    };
}

bounded_text!(
    /// Text to type into the focused field; newline and tab are allowed, other control
    /// characters are not.
    TypedText, max = 2048, Controls::NewlineAndTab, Emptiness::Refused
);
bounded_text!(
    /// A model's one-line account of what it did or asks.
    Summary, max = 1024, Controls::NewlineAndTab, Emptiness::Allowed
);
bounded_text!(
    /// One answer a model offers the user in an `Ask`. At most [`Choice::MAX_PER_ASK`] per ask.
    Choice, max = 128, Controls::None, Emptiness::Refused
);
bounded_text!(
    /// The name of a value a run extracted from the screen.
    FieldName, max = 64, Controls::None, Emptiness::Refused
);
bounded_text!(
    /// A value read off the screen. Untrusted: the run machine labels it `Untrusted(Screen)`.
    ExtractedText, max = 8192, Controls::NewlineAndTab, Emptiness::Allowed
);

impl Choice {
    /// The most choices one `Ask` may carry.
    pub const MAX_PER_ASK: usize = 8;
}
