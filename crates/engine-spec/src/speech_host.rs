//! Our own STT engine process (`speech-host`, sherpa-onnx without TTS), on the CPU.

use std::path::Path;

use crate::engine::{Engine, Launch, LaunchArgs, own, text_arg};
use crate::host_env::HostEnv;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpeechHost;

impl Engine for SpeechHost {
    fn slug(&self) -> &'static str {
        "speech_host"
    }

    fn bus_word(&self) -> &'static str {
        "speech"
    }

    fn running_word(&self) -> &'static str {
        "speech-host"
    }

    fn hears(&self) -> bool {
        true
    }

    fn speaks(&self) -> bool {
        true
    }

    /// The folder of the libraries it loads (sherpa-onnx and onnxruntime) is its library path and
    /// its sandbox reads it.
    fn env(&self, libs: Option<&Path>) -> HostEnv {
        match libs {
            Some(libs) => HostEnv {
                vars: vec![("LD_LIBRARY_PATH", libs.to_string_lossy().into_owned())],
                reads: vec![libs.to_path_buf()],
            },
            None => HostEnv::default(),
        }
    }

    fn command(&self, launch: &Launch<'_>) -> LaunchArgs {
        LaunchArgs {
            front: own(&["--model-dir"]),
            back: vec![text_arg(launch.snapshot.to_string_lossy())],
        }
    }
}
