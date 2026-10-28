//! How to start an engine: the unit the host runs.

use std::path::PathBuf;

use engine_spec::{EnginePaths, Launch, ProgramPath, engine, repo_dir, snapshot_dir};
use model_catalog::{
    ContextArg, EngineArg, EngineKind, EngineProfile, GpuNeed, MiB, ModelEntry, Tokens,
};
use serde::{Deserialize, Serialize};

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
#[non_exhaustive]
pub enum Network {
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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

/// Host memory an engine process needs beyond its weights: the interpreter, the tokenizer and
/// the staging buffers of a load. A proposal, measured by spike S1 (FINDINGS).
const HOST_MEMORY_BASE: MiB = MiB(4096);

/// The context window an engine is started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Window {
    /// The entry's `text_out.context`, the window its engine line has always served.
    #[default]
    Serve,
    /// A window the caller chose (from the model's limits and the free GPU memory).
    Chosen(Tokens),
}

impl Window {
    /// The tokens to start with: the chosen window, or the entry's served one. `None` when the
    /// entry has no chat window to serve (a transcriber's is zero).
    fn tokens(self, entry: &ModelEntry) -> Option<Tokens> {
        match self {
            Window::Chosen(tokens) => Some(tokens),
            Window::Serve => entry
                .capabilities
                .text_out
                .as_ref()
                .map(|text| text.context)
                .filter(|context| context.0 > 0),
        }
    }
}

/// Why no unit can be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("settings list no program for the {kind:?} engine")]
pub struct MissingProgram {
    pub kind: EngineKind,
}

/// Pure: the unit that runs `profile` for `entry`, listening on `socket`, with the window the
/// entry serves. See [`command_with_window`].
pub fn command(
    entry: &ModelEntry,
    profile: &EngineProfile,
    paths: &EnginePaths,
    socket: &SocketPath,
) -> Result<UnitSpec, MissingProgram> {
    command_with_window(entry, profile, paths, socket, Window::Serve)
}

/// Pure: the unit that runs `profile` for `entry`, listening on `socket`, with `window`.
///
/// A profile that names a [`ContextArg::Flag`] gets that argument and the window's tokens after
/// its listed args (a later flag wins for the engines that repeat one); `PerRequest` and no
/// `context_arg` add nothing, and neither does an entry with no window to serve.
///
/// `{socket}` in a profile's args is replaced by the socket path wherever it appears. The weights
/// are the entry's snapshot under `paths.hf_cache` (`models--<org>--<name>/snapshots/<revision>`):
/// vLLM gets the directory as `--model`, llama-server the two GGUF files, the speech host
/// `--model-dir` (each engine's own arguments are `Engine::command` in `engine-spec`); the whole
/// `models--<org>--<name>` directory (the snapshot's files are links into its `blobs/`) is the
/// sandbox's only read-only bind, and the socket's directory its only writable one. An entry whose `vram.gpu_need()` is `Absent` gets `GpuAccess::Absent` in its
/// sandbox. The program is the one `paths` lists for the profile's engine (a uv environment's
/// interpreter for vLLM and Kokoro, the interpreter in Strata's own folder, whose args name its
/// server script and flags); an engine with none listed is [`MissingProgram`].
/// Every engine starts offline (`HF_HUB_OFFLINE=1`) and serves the model under the catalog id, so
/// a request's model name does not depend on the repository it came from.
///
/// `memory_max` is host memory, not VRAM: the weights and overhead figures plus
/// [`HOST_MEMORY_BASE`], which a CPU-only entry (all-zero figures) still gets.
pub fn command_with_window(
    entry: &ModelEntry,
    profile: &EngineProfile,
    paths: &EnginePaths,
    socket: &SocketPath,
    window: Window,
) -> Result<UnitSpec, MissingProgram> {
    let repo_dir = repo_dir(&paths.hf_cache, &entry.source);
    let snapshot = snapshot_dir(&repo_dir, &entry.source);
    let socket_text = socket.0.to_string_lossy();
    let listed = profile
        .args
        .iter()
        .map(|arg| EngineArg(arg.0.replace(SOCKET_PLACEHOLDER, &socket_text)));
    let own = engine(profile.kind).command(&Launch {
        entry,
        weights: &profile.weights,
        snapshot: &snapshot,
        socket: &socket_text,
    });
    let sized = match (&profile.context_arg, window.tokens(entry)) {
        (Some(ContextArg::Flag(flag)), Some(tokens)) => {
            vec![flag.clone(), EngineArg(tokens.0.to_string())]
        }
        _ => Vec::new(),
    };
    let program = paths
        .program(profile.kind)
        .ok_or(MissingProgram { kind: profile.kind })?;
    Ok(UnitSpec {
        program: program.clone(),
        args: own
            .front
            .into_iter()
            .chain(own.back)
            .chain(listed)
            .chain(sized)
            .collect(),
        env: vec![EnvPair {
            name: "HF_HUB_OFFLINE".into(),
            value: "1".into(),
        }],
        sandbox: Sandbox {
            network: Network::None,
            read: vec![repo_dir],
            write: socket.0.parent().map(PathBuf::from).into_iter().collect(),
            gpu: match entry.vram.gpu_need() {
                GpuNeed::Absent => GpuAccess::Absent,
                GpuNeed::Needed => GpuAccess::Nvidia,
            },
            memory_max: MiB(HOST_MEMORY_BASE
                .0
                .saturating_add(entry.vram.weights.0)
                .saturating_add(entry.vram.overhead.0)),
        },
    })
}

const SOCKET_PLACEHOLDER: &str = "{socket}";
