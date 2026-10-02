//! The session value and its two transitions.

use std::collections::VecDeque;

use model_provider::{ImageInput, ModelName, TurnRequest};
use vision_prep::FrameMap;

use crate::{CuaProfile, CuaTaskText, ObservationIn, RepairBudget, StepOutcome, TurnTranscript};

/// One past step the prompt may replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryTurn {
    pub(crate) reply: TurnTranscript,
    pub(crate) frame: Option<ImageInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuaSession {
    profile: CuaProfile,
    task: CuaTaskText,
    model: ModelName,
    history: VecDeque<HistoryTurn>,
    repairs_left: RepairBudget,
}

impl CuaSession {
    pub fn begin(profile: CuaProfile, task: CuaTaskText, model: ModelName) -> CuaSession {
        let repairs_left = profile.repair;
        CuaSession {
            profile,
            task,
            model,
            history: VecDeque::new(),
            repairs_left,
        }
    }

    /// Pure prompt assembly for one step.
    pub fn request(&self, obs: &ObservationIn, map: &FrameMap, image: ImageInput) -> TurnRequest {
        let _ = (
            &self.profile,
            &self.task,
            &self.model,
            &self.history,
            obs,
            map,
            image,
        );
        todo!("CuaSession::request: system prompt, history window, observation, frame")
    }

    /// Parses the reply, maps its points into window space and pushes the step into the history.
    pub fn absorb(self, reply: TurnTranscript, map: &FrameMap) -> (CuaSession, StepOutcome) {
        let _ = (&self.repairs_left, reply, map);
        todo!("CuaSession::absorb: parse, one repair, map, history")
    }
}
