//! Tool-call delta assembly: fragments arrive per wire `index`, a call opens when its name is
//! known, its argument text accumulates, and it closes as a checked `ToolCall`.
//!
//! The mechanism ports the rules of rig's `operation/completion.rs` (`Pending`, `Arguments`,
//! `IfMalformed`) and `openai/wire/dto.rs` (`evicts`), MIT.
//!
// Portions adapted from rig (https://github.com/0xPlaygrounds/rig, crates/rig-core, commit acdcf34),
// MIT License, Copyright (c) 2024, Playgrounds Analytics Inc. See THIRD-PARTY-NOTICES.

use std::collections::BTreeSet;

use model_provider::{CallIndex, JsonText, ToolCall, ToolCallId, ToolName, TurnEvent};
use model_wire::CodecError;

/// The most argument text one call may accumulate (rig's `MAX_TOOL_INPUT_BYTES`).
pub(crate) const MAX_TOOL_INPUT_BYTES: usize = 32 << 20;

/// What closing a call does when its arguments are not valid JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IfMalformed {
    /// The provider said the call was complete, so bad arguments are a fault.
    Fail,
    /// The call was superseded mid-assembly (a gateway reused the index). Arguments that never
    /// began mean `{}`; arguments that began and are not valid JSON are a fault, never `{}`: a
    /// call is not delivered with arguments the model did not write.
    Superseded,
    /// The stream was cut: never deliver a half-formed call.
    Drop,
    /// Not decided yet: leave the call as it is. The decoder delivers calls at the finish reason,
    /// so only the assembler's own tests close with it.
    #[allow(dead_code)]
    KeepOpen,
}

/// What `close` made of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Closed {
    Done(ToolCall),
    Dropped,
    Open,
}

/// The parts of one wire fragment, with empty strings already removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Fragment {
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opened {
    No,
    Yes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overflow {
    Within,
    Over,
}

/// One call being assembled.
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    ordinal: CallIndex,
    id: Option<String>,
    name: Option<ToolName>,
    args: String,
    opened: Opened,
    overflow: Overflow,
}

impl Pending {
    pub(crate) fn new(ordinal: CallIndex) -> Self {
        Self {
            ordinal,
            id: None,
            name: None,
            args: String::new(),
            opened: Opened::No,
            overflow: Overflow::Within,
        }
    }

    pub(crate) fn ordinal(&self) -> CallIndex {
        self.ordinal
    }

    /// Whether `fragment` starts a different call than this one (a gateway reused the index): a
    /// new id, or a name with no id and no arguments after arguments have begun.
    pub(crate) fn evicted_by(&self, fragment: &Fragment) -> bool {
        let new_id = matches!((&self.id, &fragment.id), (Some(old), Some(new)) if old != new);
        let bare_opener = fragment.name.is_some()
            && fragment.id.is_none()
            && fragment.arguments.is_none()
            && self.name.is_some()
            && !self.args.is_empty();
        new_id || bare_opener
    }

    /// Takes one fragment; returns the events it caused. `taken` holds the ids of the reply so far:
    /// an id must be unique within it.
    pub(crate) fn absorb(
        &mut self,
        fragment: Fragment,
        taken: &mut BTreeSet<String>,
    ) -> Result<Vec<TurnEvent>, CodecError> {
        let mut events = Vec::new();
        if let Some(id) = fragment.id.filter(|_| self.id.is_none()) {
            self.id = Some(id);
        }
        if let Some(name) = fragment.name.filter(|_| self.name.is_none()) {
            self.name = Some(ToolName::new(name).map_err(|_| CodecError::Unreadable)?);
        }
        if let (Opened::No, Some(name)) = (self.opened, &self.name) {
            let id = self
                .id
                .clone()
                .unwrap_or_else(|| format!("call_{}", self.ordinal.0));
            if !taken.insert(id.clone()) {
                return Err(CodecError::Unreadable);
            }
            self.opened = Opened::Yes;
            self.id = Some(id.clone());
            events.push(TurnEvent::ToolCallStarted {
                index: self.ordinal,
                id: ToolCallId(id),
                name: name.clone(),
            });
            if !self.args.is_empty() && self.args != "null" {
                events.push(TurnEvent::ToolCallDelta {
                    index: self.ordinal,
                    fragment: self.args.clone(),
                });
            }
        }
        if let Some(text) = fragment.arguments {
            events.extend(self.push_arguments(text));
        }
        Ok(events)
    }

    /// Appends argument text. A `null` that arrives first is held, not delivered, and the first
    /// real fragment replaces it.
    fn push_arguments(&mut self, text: String) -> Option<TurnEvent> {
        if self.args.is_empty() && text == "null" {
            self.args = text;
            return None;
        }
        if self.args == "null" {
            self.args.clear();
        }
        if self.args.len() + text.len() > MAX_TOOL_INPUT_BYTES {
            self.overflow = Overflow::Over;
            return None;
        }
        self.args.push_str(&text);
        (self.opened == Opened::Yes).then_some(TurnEvent::ToolCallDelta {
            index: self.ordinal,
            fragment: text,
        })
    }

    /// Ends the call. Empty arguments, or a held `null`, mean `{}`.
    pub(crate) fn close(self, how: IfMalformed) -> Result<Closed, CodecError> {
        let (Some(name), Some(id)) = (self.name, self.id) else {
            return match how {
                IfMalformed::Fail => Err(CodecError::BadToolArguments),
                IfMalformed::KeepOpen => Ok(Closed::Open),
                IfMalformed::Superseded | IfMalformed::Drop => Ok(Closed::Dropped),
            };
        };
        let text = self.args.trim();
        let text = if text.is_empty() || text == "null" {
            "{}"
        } else {
            text
        };
        // Arguments are an object: valid JSON of another type (`[1]`, `"x"`, `5`) is malformed.
        let parsed = match self.overflow {
            Overflow::Within => JsonText::new(text).ok().filter(|_| text.starts_with('{')),
            Overflow::Over => None,
        };
        let input = match (parsed, how) {
            (Some(input), _) => input,
            (None, IfMalformed::Fail) => return Err(CodecError::BadToolArguments),
            (None, IfMalformed::Drop) => return Ok(Closed::Dropped),
            (None, IfMalformed::KeepOpen) => return Ok(Closed::Open),
            (None, IfMalformed::Superseded) => return Err(CodecError::BadToolArguments),
        };
        Ok(Closed::Done(ToolCall {
            id: ToolCallId(id),
            name,
            input,
        }))
    }
}

#[cfg(test)]
mod tests;
