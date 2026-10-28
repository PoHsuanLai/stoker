//! The engines that run models on this computer, as data.
//!
//! llama.cpp, vLLM and Strata each come in builds for a processor and a backend (the processor
//! alone, Vulkan, CUDA, ROCm). This crate holds what is known about them and does nothing to the
//! system: no disk, no network, no runtime. Reading the machine and its install directories is
//! `engine-probe`; fetching is `engine-install`.
//!
//! - [`Manifest`] is the pinned table: for an engine, a processor and a backend, the files to
//!   download with their hashes and sizes, the distro package where one exists, and the
//!   [`seal`] the unpacked tree must have.
//! - [`Host`] is what to choose by: the processor, the cards ([`Gpus`]) and the distro; the
//!   person may force a backend.
//! - The engine itself is the catalog's [`EngineKind`](model_catalog::EngineKind), named by
//!   `engine_spec::engine(kind).disk_word()` and [`engine_from_slug`]; its distro packages are
//!   `engine_spec::engine(kind).packages()`.
//!
//! ```
//! use engine_manifest::{Arch, Backend, Manifest};
//! use model_catalog::EngineKind;
//!
//! let manifest = Manifest::pinned();
//! let row = manifest
//!     .row(EngineKind::LlamaServer, Arch::X86_64, Backend::Vulkan)
//!     .expect("a build for Vulkan");
//! assert_eq!(row.version, "b11669");
//! ```

mod engine;
mod failure;
mod host;
mod manifest;
mod release;
mod rows;
mod seal;

pub use engine::engine_from_slug;
pub use failure::Failure;
pub use host::{Arch, Backend, BackendChoice, GpuVendor, Gpus, preference};
pub use manifest::{Artifact, Delivery, Host, Manifest, Offer, PythonEnv, Row};
pub use release::OsRelease;
pub use seal::{SealLine, TreeSeal, hex};
