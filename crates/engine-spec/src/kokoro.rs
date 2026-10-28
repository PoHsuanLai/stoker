//! Kokoro-FastAPI in a uv environment: a separate process, so its GPL espeak-ng is never linked
//! into ours.

use crate::engine::{Engine, Launch, LaunchArgs, own};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Kokoro;

impl Engine for Kokoro {
    fn slug(&self) -> &'static str {
        "kokoro_fast_api"
    }

    fn command(&self, _launch: &Launch<'_>) -> LaunchArgs {
        LaunchArgs {
            front: own(&["-m", "uvicorn", "api.src.main:app"]),
            back: Vec::new(),
        }
    }
}
