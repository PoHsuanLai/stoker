//! A new dialect needs only its impl: a test dialect that overrides one method gets the defaults
//! for the rest.

use cua_action::{ActionProfile, ModelSpace};
use cua_parse::{ParseError, ParseLimits, Parsed, ToolContext};
use cua_session::{ActionDialect, TurnTranscript};

struct Echo;

impl ActionDialect for Echo {
    fn parse(
        &self,
        _space: ModelSpace,
        _context: ToolContext,
        _reply: &TurnTranscript,
        _limits: ParseLimits,
    ) -> Result<Parsed, ParseError> {
        Err(ParseError::NoAction)
    }
}

#[test]
fn a_dialect_with_only_a_parse_takes_the_defaults() {
    let echo: &dyn ActionDialect = &Echo;
    assert_eq!(echo.wire(), None);
    assert!(!echo.is_text());
    assert!(!echo.is_tool());
    assert_eq!(
        echo.retry(ActionProfile::default()),
        "Answer again with one call to the computer tool."
    );
    assert!(echo.schema(ActionProfile::default()).is_none());
}
