//! vLLM, in a Python environment of its own.

use crate::engine::{Engine, Launch, LaunchArgs, own, text_arg};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Vllm;

impl Engine for Vllm {
    fn slug(&self) -> &'static str {
        "vllm"
    }

    fn package_paths(&self) -> &'static [&'static str] {
        &["/usr/bin/vllm", "/usr/local/bin/vllm"]
    }

    fn command(&self, launch: &Launch<'_>) -> LaunchArgs {
        LaunchArgs {
            front: own(&["-m", "vllm.entrypoints.openai.api_server", "--model"]),
            back: vec![
                text_arg(launch.snapshot.to_string_lossy()),
                text_arg("--served-model-name"),
                text_arg(launch.entry.id.0.clone()),
                text_arg("--uds"),
                text_arg(launch.socket),
            ],
        }
    }

    fn requires_gpu_estimate(&self) -> bool {
        true
    }
}
