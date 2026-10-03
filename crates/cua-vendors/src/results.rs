//! The results of the previous step as the next request's parts, one `ToolResult` per call.
//!
//! A vendor that batches runs calls in order and stops at the first failure: every call after
//! the first that was not done is `NotRun`, whatever the caller reported, with the text the
//! vendor requires (Anthropic: "Not executed: an earlier computer action in this turn failed.").

use model_provider::{ImageInput, Part, ToolCallId, ToolResult, ToolStatus};

use crate::StepResult;

/// The text of a call that was skipped because an earlier one failed.
pub(crate) const NOT_RUN: &str = "Not executed: an earlier computer action in this turn failed.";

/// What a call came to, with the stop-at-first-failure rule applied.
pub(crate) enum Outcome<'a> {
    Done(&'a ToolCallId),
    Refused(&'a ToolCallId, &'a str),
    NotRun(&'a ToolCallId),
}

pub(crate) fn outcomes(done: &[StepResult]) -> Vec<Outcome<'_>> {
    let mut stopped = false;
    done.iter()
        .map(|result| match (result, stopped) {
            (StepResult::Done(id), false) => Outcome::Done(id),
            (StepResult::Refused { id, why }, false) => {
                stopped = true;
                Outcome::Refused(id, why)
            }
            (StepResult::NotRun(id), false) => {
                stopped = true;
                Outcome::NotRun(id)
            }
            (
                StepResult::Done(id) | StepResult::Refused { id, .. } | StepResult::NotRun(id),
                true,
            ) => Outcome::NotRun(id),
        })
        .collect()
}

fn text(text: impl Into<String>) -> Part {
    Part::Text(text.into())
}

fn clean(why: &str) -> String {
    why.chars().filter(|c| !c.is_control()).take(200).collect()
}

fn result(id: &ToolCallId, status: ToolStatus, parts: Vec<Part>) -> Part {
    Part::ToolResult(ToolResult {
        id: id.clone(),
        status,
        parts,
    })
}

/// Anthropic: `OK` for a call that ran, an error for one that did not; the new screenshot rides
/// in the last result when that call ran, else after the results.
pub(crate) fn anthropic(done: &[StepResult], next: &ImageInput) -> Vec<Part> {
    let outcomes = outcomes(done);
    let last = outcomes.len().saturating_sub(1);
    let mut placed = false;
    let mut parts: Vec<Part> = outcomes
        .iter()
        .enumerate()
        .map(|(i, outcome)| match outcome {
            Outcome::Done(id) if i == last => {
                placed = true;
                result(
                    id,
                    ToolStatus::Ok,
                    vec![text("OK"), Part::Image(next.clone())],
                )
            }
            Outcome::Done(id) => result(id, ToolStatus::Ok, vec![text("OK")]),
            Outcome::Refused(id, why) => result(
                id,
                ToolStatus::Error,
                vec![text(format!("Error: {}", clean(why)))],
            ),
            Outcome::NotRun(id) => result(id, ToolStatus::Error, vec![text(NOT_RUN)]),
        })
        .collect();
    if !placed {
        parts.push(Part::Image(next.clone()));
    }
    parts
}

/// OpenAI and Gemini: every call's result carries the new screenshot (a `computer_call_output`
/// and a `function_response` each need one); a call that did not run says why before it.
pub(crate) fn with_screenshot_each(done: &[StepResult], next: &ImageInput) -> Vec<Part> {
    let shot = || Part::Image(next.clone());
    outcomes(done)
        .iter()
        .map(|outcome| match outcome {
            Outcome::Done(id) => result(id, ToolStatus::Ok, vec![text("OK"), shot()]),
            Outcome::Refused(id, why) => result(
                id,
                ToolStatus::Error,
                vec![text(format!("Error: {}", clean(why))), shot()],
            ),
            Outcome::NotRun(id) => result(id, ToolStatus::Error, vec![text(NOT_RUN), shot()]),
        })
        .collect()
}
