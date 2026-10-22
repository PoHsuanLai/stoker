//! A model trained on one reply shape (`AnswerShape`) is told that shape in the system message,
//! reminded of it when a reply has to be repaired, and its step is worded by what the shape
//! holds: the `<action>` sentence rather than the whole reasoning.

use cua_action::{ActionProfile, AnswerShape};
use cua_parse::answer_parts;

use crate::TurnTranscript;
use crate::prompt::body;

const THINK_ACTION_CALL: &str = include_str!("../prompts/answer_think_action_call.txt");
const THINK_ACTION_CALL_REMINDER: &str =
    "Keep the order: <think>...</think>, then <action>...</action>, then the <tool_call> block.";

/// The `# Response format` section of the system message, or `None` for a free reply.
/// `terminate` is the verb the model is shown for ending the run.
pub(crate) fn block(actions: ActionProfile, terminate: &str) -> Option<String> {
    match actions.answer {
        AnswerShape::Free => None,
        AnswerShape::ThinkActionCall => Some(format!(
            "# Response format\n\n{}",
            body(THINK_ACTION_CALL).replace("{terminate}", terminate)
        )),
    }
}

/// The words of the repair message about the shape of a reply; `None` for a free reply.
pub(crate) fn reminder(answer: AnswerShape) -> Option<&'static str> {
    match answer {
        AnswerShape::Free => None,
        AnswerShape::ThinkActionCall => Some(THINK_ACTION_CALL_REMINDER),
    }
}

/// The words of a step: the `<action>` sentence of a reply shaped think, action, call (else its
/// reasoning), otherwise the reasoning the server separated, otherwise the reply text. When we
/// read the calls from the text, the call blocks are not part of the words.
pub(crate) fn thought(reply: &TurnTranscript, actions: ActionProfile) -> Option<String> {
    let wrapped = match actions.answer {
        AnswerShape::Free => None,
        AnswerShape::ThinkActionCall => {
            let parts = answer_parts(&reply.text);
            parts.action.or(parts.think)
        }
    };
    let words = |text: &str| {
        let text = if actions.calls.is_read_by_us() {
            text.split("<tool_call>").next().unwrap_or_default()
        } else {
            text
        };
        Some(text.trim().to_owned()).filter(|text| !text.is_empty())
    };
    wrapped.or_else(|| {
        [&reply.thought, &reply.text]
            .into_iter()
            .find_map(|text| words(text))
    })
}
