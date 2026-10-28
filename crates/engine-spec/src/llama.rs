//! llama.cpp's `llama-server`.

use model_catalog::WeightFiles;

use crate::cache::file_name;
use crate::engine::{Engine, Launch, LaunchArgs, text_arg};
use crate::package::{PackageManager, PackageSource, Releases, Unnumbered, Verified};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Llama;

/// Debian's `forky` and `sid` and Ubuntu's `resolute` (26.04) list it, and each package holds
/// /usr/bin/llama-server (packages.debian.org, packages.ubuntu.com). Arch's `extra/llama-cpp`
/// holds /usr/bin/llama-server (archlinux.org's file list). Fedora's `llama-cpp` is in releases
/// 43 and rawhide (mdapi); its file list was not reachable.
static PACKAGES: [PackageSource; 4] = [
    PackageSource {
        manager: PackageManager::Apt,
        name: "llama.cpp-tools",
        releases: Releases::From {
            id: "debian",
            version: (14, 0),
            unnumbered: Unnumbered::New,
        },
        verified: Verified::Yes,
    },
    PackageSource {
        manager: PackageManager::Apt,
        name: "llama.cpp-tools",
        releases: Releases::From {
            id: "ubuntu",
            version: (26, 4),
            unnumbered: Unnumbered::Old,
        },
        verified: Verified::Yes,
    },
    PackageSource {
        manager: PackageManager::Dnf,
        name: "llama-cpp",
        releases: Releases::From {
            id: "fedora",
            version: (43, 0),
            unnumbered: Unnumbered::Old,
        },
        verified: Verified::No,
    },
    PackageSource {
        manager: PackageManager::Pacman,
        name: "llama-cpp",
        releases: Releases::Rolling { id: "arch" },
        verified: Verified::Yes,
    },
];

impl Engine for Llama {
    fn slug(&self) -> &'static str {
        "llama_server"
    }

    fn disk_word(&self) -> &'static str {
        "llama_cpp"
    }

    fn packages(&self) -> &'static [PackageSource] {
        &PACKAGES
    }

    fn package_paths(&self) -> &'static [&'static str] {
        &["/usr/bin/llama-server", "/usr/local/bin/llama-server"]
    }

    fn command(&self, launch: &Launch<'_>) -> LaunchArgs {
        let files = match launch.weights {
            WeightFiles::Gguf { model, mmproj } => {
                let projector = mmproj
                    .as_ref()
                    .filter(|name| !name.0.is_empty())
                    .map(|name| {
                        [
                            text_arg("--mmproj"),
                            text_arg(launch.snapshot.join(file_name(&name.0)).to_string_lossy()),
                        ]
                    });
                [
                    text_arg("--model"),
                    text_arg(launch.snapshot.join(file_name(&model.0)).to_string_lossy()),
                ]
                .into_iter()
                .chain(projector.into_iter().flatten())
                .collect()
            }
            // Not GGUF files: the catalog should not pair them (the engine refuses to start with
            // no model, which the supervisor reports as an exit).
            WeightFiles::HfSnapshot | WeightFiles::SherpaDir => Vec::new(),
        };
        let serve = [
            text_arg("--alias"),
            text_arg(launch.entry.id.0.clone()),
            text_arg("--host"),
            text_arg(launch.socket),
        ];
        LaunchArgs {
            front: Vec::new(),
            back: files.into_iter().chain(serve).collect(),
        }
    }

    fn accepts(&self, weights: &WeightFiles) -> bool {
        matches!(weights, WeightFiles::Gguf { .. })
    }
}
