//! The energy gate: speech when a frame is louder than a threshold, held for a few frames.

use serde::{Deserialize, Serialize};
use speech_provider::{Frame512, SpeechProb, VoiceActivity, Voiced};

use crate::Level;

/// A number of 32 ms frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameCount(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyGateParams {
    /// A frame at or above this level is speech.
    pub threshold: Level,
    /// Frames still called speech after the last loud one.
    pub hangover: FrameCount,
}

impl Default for EnergyGateParams {
    /// The proposal: -40 dBFS (a level of 333) and 8 frames (256 ms). The `voice` settings may
    /// replace both.
    fn default() -> Self {
        Self {
            threshold: Level(333),
            hangover: FrameCount(8),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnergyGate {
    params: EnergyGateParams,
    since_loud: FrameCount,
}

impl EnergyGate {
    pub fn new(params: EnergyGateParams) -> Self {
        Self {
            params,
            since_loud: FrameCount(u16::MAX),
        }
    }

    pub fn params(&self) -> &EnergyGateParams {
        &self.params
    }
}

impl VoiceActivity for EnergyGate {
    fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        let _ = (&self.params, &mut self.since_loud, frame);
        todo!("EnergyGate::push: level_of against the threshold, hangover frames")
    }

    fn reset(&mut self) {
        todo!("EnergyGate::reset: no loud frame seen")
    }
}
