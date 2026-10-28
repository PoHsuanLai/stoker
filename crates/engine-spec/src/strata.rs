//! Strata, run from its own folder with the Python environment in it. The profile's args name
//! its server script and flags, so the engine adds none.

use crate::engine::{Engine, Pointed};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Strata;

impl Engine for Strata {
    fn slug(&self) -> &'static str {
        "strata"
    }

    fn pointed(&self) -> Option<Pointed> {
        Some(Pointed {
            program: ".venv/bin/python",
            marker: "serve/server.py",
            build_record: "engine/BUILD.json",
        })
    }
}
