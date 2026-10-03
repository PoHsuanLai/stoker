//! Vendor safety hints may only add asks: a confirmation request becomes an `Ask` ahead of the
//! actions, and a block removes the actions and ends the run as infeasible. Nothing a vendor says
//! can take an ask away or make an action more permitted.

use cua_action::{CoordSpace, CuaAction, FinishOutcome, Summary};
use cua_parse::InSpace;
use model_provider::SafetySignal;

/// Text of a hint as a `Summary`: control characters dropped, cut to the limit.
fn summary(prefix: &str, text: &str) -> Summary {
    let text: String = format!("{prefix}{text}")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(Summary::MAX_CHARS as usize)
        .collect();
    Summary::new(text).unwrap_or_else(|_| unreachable!("the text is cut and cleaned"))
}

fn apply<S: CoordSpace>(actions: Vec<CuaAction<S>>, safety: &[SafetySignal]) -> Vec<CuaAction<S>> {
    let blocked = safety.iter().find_map(|s| match s {
        SafetySignal::Blocked(why) => Some(why),
        SafetySignal::RequireConfirmation(_) => None,
    });
    if let Some(why) = blocked {
        return vec![CuaAction::Finish {
            outcome: FinishOutcome::Infeasible,
            summary: summary("The model's provider blocked this: ", why),
            extracted: Vec::new(),
        }];
    }
    let asks = safety.iter().filter_map(|s| match s {
        SafetySignal::RequireConfirmation(what) => Some(CuaAction::Ask {
            question: summary("The model's provider asks you to confirm: ", what),
            choices: Vec::new(),
        }),
        SafetySignal::Blocked(_) => None,
    });
    asks.chain(actions).collect()
}

pub(crate) fn apply_safety(actions: InSpace, safety: &[SafetySignal]) -> InSpace {
    match actions {
        InSpace::Image(list) => InSpace::Image(apply(list, safety)),
        InSpace::Grid(max, list) => InSpace::Grid(max, apply(list, safety)),
    }
}
