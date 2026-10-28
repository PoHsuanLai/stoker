//! vLLM, in a Python environment of its own.

use std::path::Path;

use crate::engine::{Engine, Launch, LaunchArgs, own, text_arg};
use crate::host_env::HostEnv;

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

    fn seat_flags(&self) -> Option<&'static [&'static str]> {
        Some(&["--max-num-seqs"])
    }

    /// An environment of ours is sealed with its bytecode caches in it, so Python must not write
    /// more of them there (and reads of a changed one are caught before the start).
    fn env(&self, _libs: Option<&Path>) -> HostEnv {
        HostEnv {
            vars: vec![("PYTHONDONTWRITEBYTECODE", "1".to_owned())],
            reads: Vec::new(),
        }
    }

    fn requires_gpu_estimate(&self) -> bool {
        true
    }
}
