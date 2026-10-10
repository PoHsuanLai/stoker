//! The model side of one computer-use step, as a pure value.
//!
//! `request` assembles the prompt for a model from the goal, the history and a prepared frame;
//! `absorb` takes the model's reply, parses it (one repair is allowed), maps its points into
//! window space and pushes the step into the history. Nothing here waits or does I/O.
//!
//! ```
//! use cua_action::{CuaDialect, GridMax, ModelSpace, ToolDialect};
//! use cua_session::{CuaProfile, CuaSession, CuaTaskText, FrameBudget, RepairBudget};
//! use model_provider::ModelName;
//! use vision_prep::{Encoding, PatchFactor, PixelCount, ResizeRule};
//!
//! let profile = CuaProfile {
//!     dialect: CuaDialect::Tool(ToolDialect::Holo31),
//!     rule: ResizeRule::SmartResize {
//!         factor: PatchFactor(32),
//!         min_pixels: PixelCount(65_536),
//!         max_pixels: PixelCount(16_777_216),
//!     },
//!     space: ModelSpace::Grid(GridMax(1000)),
//!     history: FrameBudget(3),
//!     repair: RepairBudget(1),
//!     encoding: Encoding::Png,
//! };
//! let task = CuaTaskText { goal: "save the file".into(), hints: vec![] };
//! let session = CuaSession::begin(profile, task, ModelName("holo".into()));
//! // A new session remembers no step and holds its whole repair budget.
//! assert_eq!(session.remembered(), 0);
//! assert_eq!(session.repairs_left(), RepairBudget(1));
//! ```

mod history;
mod model;
mod prompt;
mod reply;
mod session;
mod transcript;
mod window;

pub use model::{
    CuaProfile, CuaTaskText, FrameBudget, MaskedRegions, ObservationIn, RepairBudget, StepIndex,
    StepLines, StepNote, StepOutcome, TreeText, TurnSettings, TurnTranscript,
};
pub use session::CuaSession;
pub use transcript::TranscriptSink;
