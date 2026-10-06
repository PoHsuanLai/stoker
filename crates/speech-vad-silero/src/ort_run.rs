//! The ONNX Runtime session over silero_vad.onnx, through `ort` with `load-dynamic`: no native
//! library is linked or downloaded at build time; libonnxruntime is opened at the first `load`
//! from `ORT_DYLIB_PATH` (the daemon sets it), else the platform's default lookup.
//!
//! Inputs `input` [1, 576] f32, `state` [2, 1, 128] f32, `sr` i64 scalar; outputs `output`
//! [1, 1] and `stateN` [2, 1, 128]. (tract was tried first and cannot load this graph: it
//! analyses both arms of the model's `If (sr == 16000)` and of the data-dependent `If`s inside,
//! whose shapes it cannot reconcile; see FINDINGS.md.)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::session::Session;
use ort::value::Tensor;

use crate::core::{INPUT_SAMPLES, Infer, Input, STATE_LEN, State};

pub struct OrtInfer {
    session: Session,
}

impl std::fmt::Debug for OrtInfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OrtInfer")
    }
}

impl OrtInfer {
    /// Opens libonnxruntime (see [`dylib_path`]) and the model. Every failure, a missing or
    /// unloadable library included, is an `Err`: `ort`'s own lazy loader panics, so the library
    /// is opened here first through the fallible `ort::init_from`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let env = std::env::var("ORT_DYLIB_PATH").ok();
        Self::load_with(&dylib_path(env.as_deref()), path)
    }

    pub fn load_with(dylib: &Path, model: &Path) -> Result<Self, String> {
        if dylib.components().count() > 1 && !dylib.is_file() {
            return Err(format!("libonnxruntime not found at {}", dylib.display()));
        }
        open_library(dylib)?;
        Session::builder()
            .and_then(|b| b.with_intra_threads(1)?.commit_from_file(model))
            .map(|session| Self { session })
            .map_err(|e| e.to_string())
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

/// The library to open: the `ORT_DYLIB_PATH` value if it is set and not empty, else the platform's
/// default file name (found by the dynamic loader's search).
pub fn dylib_path(env: Option<&str>) -> PathBuf {
    match env {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from(DEFAULT_DYLIB),
    }
}

#[cfg(target_os = "macos")]
const DEFAULT_DYLIB: &str = "libonnxruntime.dylib";
#[cfg(target_os = "windows")]
const DEFAULT_DYLIB: &str = "onnxruntime.dll";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const DEFAULT_DYLIB: &str = "libonnxruntime.so";

static LIBRARY: OnceLock<Result<(), String>> = OnceLock::new();

/// Opens libonnxruntime once per process and remembers the outcome. `ort` cannot retry: after a
/// failed open its internal once-cell is marked complete with no library in it, so a second
/// `init_from` reads uninitialised memory. The first path wins, as it does inside `ort`.
fn open_library(dylib: &Path) -> Result<(), String> {
    LIBRARY
        .get_or_init(|| {
            ort::init_from(dylib)
                .map(|builder| {
                    builder.commit();
                })
                .map_err(|e| e.to_string())
        })
        .clone()
}
