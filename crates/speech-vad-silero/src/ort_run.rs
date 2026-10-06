//! The ONNX Runtime session over silero_vad.onnx, through `ort` with `load-dynamic`: no native
//! library is linked or downloaded at build time; libonnxruntime is opened at the first `load`
//! from `ORT_DYLIB_PATH` (the daemon sets it), else the platform's default lookup.
//!
//! Inputs `input` [1, 576] f32, `state` [2, 1, 128] f32, `sr` i64 scalar; outputs `output`
//! [1, 1] and `stateN` [2, 1, 128]. (tract was tried first and cannot load this graph: it
//! analyses both arms of the model's `If (sr == 16000)` and of the data-dependent `If`s inside,
//! whose shapes it cannot reconcile; see FINDINGS.md.)

use std::path::Path;

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
    pub fn load(path: &Path) -> Result<Self, String> {
        Session::builder()
            .and_then(|mut b| b.with_intra_threads(1)?.commit_from_file(path))
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
