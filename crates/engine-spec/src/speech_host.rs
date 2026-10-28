//! Our own STT engine process (`speech-host`, sherpa-onnx without TTS), on the CPU.

use crate::engine::{Engine, Launch, LaunchArgs, own, text_arg};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpeechHost;

impl Engine for SpeechHost {
    fn slug(&self) -> &'static str {
        "speech_host"
    }

    fn command(&self, launch: &Launch<'_>) -> LaunchArgs {
        LaunchArgs {
            front: own(&["--model-dir"]),
            back: vec![text_arg(launch.snapshot.to_string_lossy())],
        }
    }
}
