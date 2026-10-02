//! Silero VAD v6.2.1 (MIT, ONNX) as a [`VoiceActivity`]: 16 kHz, 512-sample frames, an LSTM
//! state of shape [2, 1, 128] carried from frame to frame. The weights file is read from a path
//! the daemon passes in (the weights are not in the repository).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use speech_provider::{Frame512, SpeechProb, VoiceActivity, Voiced};

/// Where the Silero ONNX file is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SileroModelPath(pub PathBuf);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SileroConfig {
    pub model: SileroModelPath,
    /// A frame at or above this probability is speech.
    pub threshold: SpeechProb,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SileroError {
    #[error("the model file cannot be read")]
    ModelMissing,
    #[error("the runtime refused the model: {0}")]
    Runtime(String),
}

/// A loaded model and its recurrent state.
#[derive(Debug)]
pub struct SileroVad {
    threshold: SpeechProb,
}

impl SileroVad {
    pub fn load(config: &SileroConfig) -> Result<Self, SileroError> {
        let _ = config;
        todo!("SileroVad::load: ort session over the ONNX file, zeroed [2,1,128] state")
    }
}

impl VoiceActivity for SileroVad {
    fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        let _ = (&self.threshold, frame);
        todo!(
            "SileroVad::push: run the model on the frame, carry the state, threshold the probability"
        )
    }

    fn reset(&mut self) {
        todo!("SileroVad::reset: zero the recurrent state")
    }
}
