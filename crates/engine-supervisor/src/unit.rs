//! How to start an engine: the unit the host runs.

use std::path::{Path, PathBuf};

use model_catalog::{
    EngineArg, EngineKind, EngineProfile, GpuNeed, MiB, ModelEntry, WeightFiles, WeightSource,
};
use serde::{Deserialize, Serialize};

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

/// Host memory an engine process needs beyond its weights: the interpreter, the tokenizer and
/// the staging buffers of a load. A proposal, measured by spike S1 (FINDINGS).
const HOST_MEMORY_BASE: MiB = MiB(4096);

/// Pure: the unit that runs `profile` for `entry`, listening on `socket`.
///
/// `{socket}` in a profile's args is replaced by the socket path wherever it appears. The weights
/// are the entry's snapshot under `paths.hf_cache` (`models--<org>--<name>/snapshots/<revision>`):
/// vLLM gets the directory as `--model`, llama-server the two GGUF files, the speech host
/// `--model-dir`; the whole `models--<org>--<name>` directory (the snapshot's files are links into
/// its `blobs/`) is the sandbox's only read-only bind, and the socket's directory its only
/// writable one. An entry whose `vram.gpu_need()` is `Absent` gets `GpuAccess::Absent` in its
/// sandbox; `SpeechHost` runs `paths.speech_host` and `KokoroFastApi` runs `paths.kokoro_python`
/// (a uv environment's interpreter, like vLLM's). Every engine starts offline
/// (`HF_HUB_OFFLINE=1`) and serves the model under the catalog id, so a request's model name does
/// not depend on the repository it came from.
///
/// `memory_max` is host memory, not VRAM: the weights and overhead figures plus
/// [`HOST_MEMORY_BASE`], which a CPU-only entry (all-zero figures) still gets.
pub fn command(
    entry: &ModelEntry,
    profile: &EngineProfile,
    paths: &EnginePaths,
    socket: &SocketPath,
) -> UnitSpec {
    let repo_dir = repo_dir(&paths.hf_cache, &entry.source);
    let snapshot = snapshot_dir(&repo_dir, &entry.source);
    let socket_text = socket.0.to_string_lossy();
    let own = |args: &[&str]| -> Vec<EngineArg> {
        args.iter().map(|a| EngineArg((*a).to_owned())).collect()
    };
    let listed = profile
        .args
        .iter()
        .map(|arg| EngineArg(arg.0.replace(SOCKET_PLACEHOLDER, &socket_text)));
    let path_arg = |path: PathBuf| EngineArg(path.to_string_lossy().into_owned());
    let (program, front, back) = match (profile.kind, &profile.weights) {
        (EngineKind::Vllm, _) => (
            paths.vllm_python.clone(),
            own(&["-m", "vllm.entrypoints.openai.api_server", "--model"]),
            vec![
                path_arg(snapshot),
                EngineArg("--served-model-name".into()),
                EngineArg(entry.id.0.clone()),
                EngineArg("--uds".into()),
                EngineArg(socket_text.clone().into_owned()),
            ],
        ),
        (EngineKind::LlamaServer, weights) => {
            let files = match weights {
                WeightFiles::Gguf { model, mmproj } => {
                    let projector = mmproj
                        .as_ref()
                        .filter(|name| !name.0.is_empty())
                        .map(|name| {
                            [
                                EngineArg("--mmproj".into()),
                                path_arg(snapshot.join(file_name(&name.0))),
                            ]
                        });
                    [
                        EngineArg("--model".into()),
                        path_arg(snapshot.join(file_name(&model.0))),
                    ]
                    .into_iter()
                    .chain(projector.into_iter().flatten())
                    .collect()
                }
                // Not GGUF files: the catalog should not pair them (the engine refuses to start
                // with no model, which the supervisor reports as an exit).
                WeightFiles::HfSnapshot | WeightFiles::SherpaDir => Vec::new(),
            };
            let serve = [
                EngineArg("--alias".into()),
                EngineArg(entry.id.0.clone()),
                EngineArg("--host".into()),
                EngineArg(socket_text.clone().into_owned()),
            ];
            (
                paths.llama_server.clone(),
                Vec::new(),
                files.into_iter().chain(serve).collect(),
            )
        }
        (EngineKind::SpeechHost, _) => (
            paths.speech_host.clone(),
            own(&["--model-dir"]),
            vec![path_arg(snapshot)],
        ),
        (EngineKind::KokoroFastApi, _) => (
            paths.kokoro_python.clone(),
            own(&["-m", "uvicorn", "api.src.main:app"]),
            Vec::new(),
        ),
    };
    UnitSpec {
        program,
        args: front.into_iter().chain(back).chain(listed).collect(),
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
    }
}

const SOCKET_PLACEHOLDER: &str = "{socket}";

/// One path component: the characters a repository or revision name is made of, nothing that
/// could leave the directory it is joined to.
fn component(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_start_matches('.')
        .to_owned()
}

/// A file name with any directory part dropped.
fn file_name(name: &str) -> String {
    component(name.rsplit(['/', '\\']).next().unwrap_or_default())
}

/// `<hf_cache>/models--<org>--<name>`: the cache's directory for one repository.
fn repo_dir(hf_cache: &Path, source: &WeightSource) -> PathBuf {
    let WeightSource::HuggingFace { repo, .. } = source;
    let parts: Vec<String> = repo.0.split('/').map(component).collect();
    hf_cache.join(format!("models--{}", parts.join("--")))
}

/// `<repo dir>/snapshots/<revision>`: one exact set of files.
fn snapshot_dir(repo_dir: &Path, source: &WeightSource) -> PathBuf {
    let WeightSource::HuggingFace { revision, .. } = source;
    repo_dir.join("snapshots").join(component(&revision.0))
}
