//! The ONNX Runtime session over silero_vad.onnx, through `ort` with `load-dynamic`: no native
//! library is linked or downloaded at build time; libonnxruntime is opened at the first `load`
//! from the path the caller passes (a daemon reads `ORT_DYLIB_PATH` and hands it in; this library
//! reads no environment variable), else the platform's default lookup.
//!
//! Inputs `input` [1, 576] f32, `state` [2, 1, 128] f32, `sr` i64 scalar; outputs `output`
//! [1, 1] and `stateN` [2, 1, 128]. (tract was tried first and cannot load this graph: it
//! analyses both arms of the model's `If (sr == 16000)` and of the data-dependent `If`s inside,
//! whose shapes it cannot reconcile; see FINDINGS.md.)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::session::Session;
use ort::value::Tensor;
use serde::{Deserialize, Serialize};

use crate::core::{INPUT_SAMPLES, Infer, Input, STATE_LEN, State};

/// Where libonnxruntime is: a file path, or the bare platform file name for the dynamic loader's
/// own search. The caller decides (a bin reads `ORT_DYLIB_PATH`); the library never reads the
/// environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OnnxRuntimePath(pub PathBuf);

impl OnnxRuntimePath {
    /// The value of `ORT_DYLIB_PATH` as the caller read it: used if set and not empty, else the
    /// platform's default file name.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value {
            Some(p) if !p.is_empty() => Self(PathBuf::from(p)),
            _ => Self::platform_default(),
        }
    }

    pub fn platform_default() -> Self {
        Self(PathBuf::from(DEFAULT_DYLIB))
    }
}

/// Why the runtime or the model could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrtError {
    #[error("libonnxruntime not found at {}", .0.display())]
    LibraryMissing(PathBuf),
    #[error("libonnxruntime could not be opened: {0}")]
    LibraryOpen(String),
    #[error("the model could not be loaded: {0}")]
    Model(String),
}

pub struct OrtInfer {
    session: Session,
}

impl std::fmt::Debug for OrtInfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OrtInfer")
    }
}

impl OrtInfer {
    /// Opens libonnxruntime at `dylib` and the model at `model`. Every failure, a missing or
    /// unloadable library included, is an `Err`: `ort`'s own lazy loader panics, so the library
    /// is opened here first through the fallible `ort::init_from`.
    pub fn load(dylib: &OnnxRuntimePath, model: &Path) -> Result<Self, OrtError> {
        let dylib = dylib.0.as_path();
        if dylib.components().count() > 1 && !dylib.is_file() {
            return Err(OrtError::LibraryMissing(dylib.to_path_buf()));
        }
        open_library(dylib)?;
        Session::builder()
            .and_then(|b| b.with_intra_threads(1)?.commit_from_file(model))
            .map(|session| Self { session })
            .map_err(|e| OrtError::Model(e.to_string()))
    }

    fn step(&mut self, input: &Input, state: &State) -> ort::Result<(f32, State)> {
        let x = Tensor::from_array(([1_usize, INPUT_SAMPLES], input.to_vec()))?;
        let h = Tensor::from_array(([2_usize, 1, STATE_LEN / 2], state.to_vec()))?;
        let sr = Tensor::from_array((Vec::<usize>::new(), vec![16_000_i64]))?;
        let out = self
            .session
            .run(ort::inputs!["input" => x, "state" => h, "sr" => sr])?;
        let (_, p) = out["output"].try_extract_tensor::<f32>()?;
        let (_, s) = out["stateN"].try_extract_tensor::<f32>()?;
        let mut next = [0.0_f32; STATE_LEN];
        for (slot, v) in next.iter_mut().zip(s) {
            *slot = *v;
        }
        Ok((p.first().copied().unwrap_or(f32::NAN), next))
    }
}

impl Infer for OrtInfer {
    fn run(&mut self, input: &Input, state: &State) -> Option<(f32, State)> {
        self.step(input, state).ok()
    }
}

#[cfg(target_os = "macos")]
const DEFAULT_DYLIB: &str = "libonnxruntime.dylib";
#[cfg(target_os = "windows")]
const DEFAULT_DYLIB: &str = "onnxruntime.dll";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const DEFAULT_DYLIB: &str = "libonnxruntime.so";

static LIBRARY: OnceLock<Result<(), OrtError>> = OnceLock::new();

/// Opens libonnxruntime once per process and remembers the outcome. The guard is process-wide
/// because `ort`'s own runtime is: it cannot hold two libraries. `ort` cannot retry: after a
/// failed open its internal once-cell is marked complete with no library in it, so a second
/// `init_from` reads uninitialised memory. The first path wins, as it does inside `ort`.
fn open_library(dylib: &Path) -> Result<(), OrtError> {
    LIBRARY
        .get_or_init(|| {
            ort::init_from(dylib)
                .map(|builder| {
                    builder.commit();
                })
                .map_err(|e| OrtError::LibraryOpen(e.to_string()))
        })
        .clone()
}
