//! How to start an engine: the unit the host runs.

use std::path::PathBuf;

use model_catalog::{EngineProfile, MiB, ModelEntry};
use serde::{Deserialize, Serialize};

use model_catalog::EngineArg;

/// An absolute path to an engine's program.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProgramPath(pub PathBuf);

/// The Unix socket an engine listens on, `$XDG_RUNTIME_DIR/inferd/<engine>.sock`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SocketPath(pub PathBuf);

/// One environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvPair {
    pub name: String,
    pub value: String,
}

/// What an engine may reach. Engines listen on a Unix socket only, so there is one network mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuAccess {
    Nvidia,
    Absent,
}

/// The confinement of one engine: a transient systemd user unit with `PrivateNetwork`,
/// read-only home, the weights bound read-only, device access to the GPU and a memory cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sandbox {
    pub network: Network,
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub gpu: GpuAccess,
    pub memory_max: MiB,
}

/// What the host starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSpec {
    pub program: ProgramPath,
    pub args: Vec<EngineArg>,
    pub env: Vec<EnvPair>,
    pub sandbox: Sandbox,
}

/// Where the engines' programs and the weights cache are, from settings
/// (`ai.engine.vllm.python`, `ai.engine.llama_server.path`, `ai.engine.speech_host.path`,
/// `ai.engine.kokoro.python`); never from the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnginePaths {
    pub vllm_python: ProgramPath,
    pub llama_server: ProgramPath,
    pub speech_host: ProgramPath,
    pub kokoro_python: ProgramPath,
    pub hf_cache: PathBuf,
}

/// Pure: the unit that runs `profile` for `entry`, listening on `socket`.
///
/// `{socket}` in a profile's args is replaced by the socket path. An entry whose
/// `vram.gpu_need()` is `Absent` gets `GpuAccess::Absent` in its sandbox; `SpeechHost` runs
/// `paths.speech_host` and `KokoroFastApi` runs `paths.kokoro_python` (a uv environment's
/// interpreter, like vLLM's).
pub fn command(
    entry: &ModelEntry,
    profile: &EngineProfile,
    paths: &EnginePaths,
    socket: &SocketPath,
) -> UnitSpec {
    let _ = (entry, profile, paths, socket);
    todo!("command: program, args from the profile, socket flag, HF_HUB_OFFLINE, sandbox")
}
