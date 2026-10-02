//! Units: every number that means something has a type.

use serde::{Deserialize, Serialize};

macro_rules! unit {
    ($(#[$doc:meta])* $name:ident($inner:ty)) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    };
}

unit!(
    /// A number of model tokens.
    Tokens(u32)
);
unit!(
    /// Thousandths: a sampling temperature of 0.7 is `Milli(700)`.
    Milli(u16)
);
unit!(
    /// A number of images.
    ImageCount(u16)
);
unit!(
    /// Seconds a server asks a client to wait before retrying.
    RetrySeconds(u32)
);
unit!(
    /// The position of one tool call among those a turn makes.
    CallIndex(u16)
);
