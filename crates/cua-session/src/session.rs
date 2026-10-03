//! The session value and its transitions.

use std::collections::VecDeque;

use cua_action::{CuaDialect, TextDialect};
use cua_parse::{DropReason, ParseLimits, Parsed};
use model_provider::{
    ImageInput, Message, ModelName, OutputShape, Part, Role, ToolChoice, TurnRequest,
};
use vision_prep::FrameMap;

use crate::history::{self, HistoryTurn};
use crate::prompt::{self, Fault};
use crate::{
    CuaProfile, CuaTaskText, MaskedRegions, ObservationIn, RepairBudget, StepIndex, StepOutcome,
    TurnSettings, TurnTranscript, reply, window,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuaSession {
    profile: CuaProfile,
    task: CuaTaskText,
    model: ModelName,
    settings: TurnSettings,
    history: VecDeque<HistoryTurn>,
    /// Steps taken so far, repaired or not.
    taken: u32,
    repairs_left: RepairBudget,
}

impl CuaSession {
    pub fn begin(profile: CuaProfile, task: CuaTaskText, model: ModelName) -> CuaSession {
        let repairs_left = profile.repair;
        CuaSession {
            profile,
            task,
            model,
            settings: TurnSettings::default(),
            history: VecDeque::new(),
            taken: 0,
            repairs_left,
        }
    }

    /// The same session with these request settings (sampling, output limit, reasoning, how many
    /// earlier steps are listed). The daemon takes them from the model's catalog entry.
    pub fn with_settings(self, settings: TurnSettings) -> CuaSession {
        CuaSession { settings, ..self }
    }

    pub fn repairs_left(&self) -> RepairBudget {
        self.repairs_left
    }

    /// How many earlier steps the session remembers.
    pub fn remembered(&self) -> usize {
        self.history.len()
    }

    /// Pure prompt assembly for one step: the system prompt and tool declaration of the dialect,
    /// the last few frames, the lines about earlier steps and the last results, and this frame.
    /// Points are read in the map's model space.
    pub fn request(&self, obs: &ObservationIn, map: &FrameMap, image: ImageInput) -> TurnRequest {
        self.build(obs, map, Some(image))
    }

    fn build(&self, obs: &ObservationIn, map: &FrameMap, image: Option<ImageInput>) -> TurnRequest {
        let dialect = self.profile.dialect;
        let mut messages: Vec<Message> = prompt::system_text(dialect, map, &self.task)
            .map(|text| Message {
                role: Role::System,
                parts: vec![Part::Text(text)],
            })
            .into_iter()
            .collect();
        let text_dialect = matches!(dialect, CuaDialect::Text(TextDialect::UiTars15));
        let mut lead = prompt::observation_lines(
            dialect,
            &self.task,
            &self.history,
            self.settings.lines,
            obs,
            map,
        );
        let mut parts = Vec::new();
        for turn in self.history.iter().filter(|t| t.frame.is_some()) {
            let Some(frame) = &turn.frame else { continue };
            if text_dialect {
                // The dialect's own shape: a frame, then what the model answered to it.
                messages.push(Message {
                    role: Role::User,
                    parts: vec![Part::Image(frame.clone())],
                });
                messages.push(Message {
                    role: Role::Assistant,
                    parts: vec![Part::Text(turn.reply.clone())],
                });
            } else {
                parts.push(Part::Text(format!(
                    "Earlier screenshot (step {}):",
                    turn.number
                )));
                parts.push(Part::Image(frame.clone()));
            }
        }
        lead.push(format!("Step {}. Screenshot:", obs.step.0));
        let mut user = vec![Part::Text(lead.join("\n"))];
        user.extend(parts);
        user.extend(image.map(Part::Image));
        messages.push(Message {
            role: Role::User,
            parts: user,
        });
        TurnRequest {
            model: self.model.clone(),
            messages,
            tools: prompt::tools(dialect, map),
            tool_choice: ToolChoice::Auto,
            tool_calls: self.settings.tool_calls,
            output: OutputShape::Free,
            limits: self.settings.limits.clone(),
            sampling: self.settings.sampling,
            reasoning: self.settings.reasoning,
            engine: self.settings.engine,
        }
    }

    /// Parses the reply, maps its points into window space and pushes the step into the history,
    /// as [`absorb_for`](Self::absorb_for) does, except that the session has not been told which
    /// request the reply answers: a repair is built from the session's own state and carries no
    /// frame, and the step is remembered without its frame. Prefer `absorb_for`.
    pub fn absorb(self, reply: TurnTranscript, map: &FrameMap) -> (CuaSession, StepOutcome) {
        let obs = ObservationIn {
            step: StepIndex(self.taken),
            cursor: None,
            prev: Vec::new(),
            masked: MaskedRegions(0),
        };
        let sent = self.build(&obs, map, None);
        self.absorb_for(&sent, reply, map)
    }

    /// Parses the reply to `sent` (the request that was sent: the one `request` built, or the
    /// repair this returned before), maps its points into window space and pushes the step into
    /// the history with the frame `sent` carried.
    ///
    /// A reply that does not parse, or whose actions are all refused, is answered with one
    /// `Repair` per unit of the profile's repair budget (the same frame, a message naming what
    /// was wrong and never repeating the reply); with none left it is `Unparseable`, or the
    /// refused actions with their reasons. Either way the step counts and is remembered.
    pub fn absorb_for(
        mut self,
        sent: &TurnRequest,
        reply: TurnTranscript,
        map: &FrameMap,
    ) -> (CuaSession, StepOutcome) {
        let dialect = self.profile.dialect;
        let parsed = reply::parse(dialect, map.space, &reply, ParseLimits::default());
        let (fault, spent) = match parsed {
            Ok(Parsed {
                thought,
                actions,
                dropped,
            }) => {
                let lines = history::phrases(&actions);
                let (mapped, out_of_frame) = window::to_window(actions, map);
                let mut dropped = dropped;
                dropped.extend(out_of_frame);
                if mapped.is_empty() && !dropped.is_empty() && self.repairs_left.0 > 0 {
                    let reasons = dropped
                        .iter()
                        .map(|d| d.reason)
                        .collect::<Vec<DropReason>>();
                    (Fault::Refused(reasons), None)
                } else {
                    let thought = thought.or_else(|| thought_of(&reply));
                    self.remember(sent, &reply, lines);
                    return (
                        self,
                        StepOutcome::Actions {
                            thought,
                            actions: mapped,
                            dropped,
                        },
                    );
                }
            }
            Err(error) if self.repairs_left.0 > 0 => (Fault::Unparsed(error), None),
            Err(error) => (Fault::Unparsed(error), Some(error)),
        };
        match spent {
            None => {
                self.repairs_left = RepairBudget(self.repairs_left.0.saturating_sub(1));
                let repair = self.repair(sent, &reply, &fault);
                (self, StepOutcome::Repair(repair))
            }
            Some(error) => {
                self.remember(sent, &reply, Vec::new());
                (self, StepOutcome::Unparseable(error))
            }
        }
    }

    /// `sent`, then (for the text dialect, whose reply is the action text) what the model wrote,
    /// then the fixed repair message.
    fn repair(&self, sent: &TurnRequest, reply: &TurnTranscript, fault: &Fault) -> TurnRequest {
        let dialect = self.profile.dialect;
        let mut request = sent.clone();
        if matches!(dialect, CuaDialect::Text(_)) && !reply.text.is_empty() {
            request.messages.push(Message {
                role: Role::Assistant,
                parts: vec![Part::Text(history::cut(&reply.text))],
            });
        }
        request.messages.push(Message {
            role: Role::User,
            parts: vec![Part::Text(prompt::repair_text(dialect, fault))],
        });
        request
    }

    fn remember(&mut self, sent: &TurnRequest, reply: &TurnTranscript, actions: Vec<String>) {
        let turn = HistoryTurn {
            number: self.taken,
            actions,
            reply: history::cut(&reply.text),
            frame: last_image(sent),
        };
        history::push(
            &mut self.history,
            turn,
            self.profile.history,
            self.settings.lines,
        );
        self.taken += 1;
        self.repairs_left = self.profile.repair;
    }
}

/// The last image a request carries: the frame of the step it asked about.
fn last_image(request: &TurnRequest) -> Option<ImageInput> {
    request
        .messages
        .iter()
        .rev()
        .flat_map(|m| m.parts.iter().rev())
        .find_map(|part| match part {
            Part::Image(image) => Some(image.clone()),
            _ => None,
        })
}

fn thought_of(reply: &TurnTranscript) -> Option<String> {
    [&reply.thought, &reply.text]
        .into_iter()
        .find(|text| !text.trim().is_empty())
        .map(|text| text.trim().to_owned())
}
