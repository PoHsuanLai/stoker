//! Silero VAD v6.2.1 (MIT, ONNX) as a [`VoiceActivity`]: 16 kHz, 512-sample frames, an LSTM
//! state of shape [2, 1, 128] carried from frame to frame, and the 64-sample context the v5+
//! model expects in front of every frame (snakers4/silero-vad `utils_vad.py`, `OnnxWrapper`).
//! The weights file is read from a path the daemon passes in (the weights are not in the
//! repository). The runtime is `ort` (load-dynamic); the model-dependent part sits behind the
//! small [`Infer`] seam so the state plumbing is tested without weights.

mod core;
mod ort_run;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use speech_provider::{Frame512, SpeechProb, VoiceActivity, Voiced};

use crate::core::Detector;
use crate::ort_run::OrtInfer;

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
    detector: Detector<OrtInfer>,
}

impl SileroVad {
    pub fn load(config: &SileroConfig) -> Result<Self, SileroError> {
        let path = &config.model.0;
        if !path.is_file() {
            return Err(SileroError::ModelMissing);
        }
        let infer = OrtInfer::load(path).map_err(SileroError::Runtime)?;
        Ok(Self {
            detector: Detector::new(infer, config.threshold),
        })
    }
}

impl VoiceActivity for SileroVad {
    fn push(&mut self, frame: &Frame512) -> (Voiced, SpeechProb) {
        self.detector.push(frame)
    }

    fn reset(&mut self) {
        self.detector.reset();
    }
}

#[cfg(test)]
mod tests;
