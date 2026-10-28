//! Which engines are on this computer already, and where from. Detection only reads files: it
//! never runs a program it finds.

use crate::layout::{InstallDir, Record, Roots, Scope};
use engine_manifest::Manifest;
use engine_spec::{ENGINES, engine};
use model_catalog::EngineKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Where a found program came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A distro package, at a fixed system path.
    Package,
    /// An install of ours for everyone.
    System,
    /// An install of ours for this person.
    Person,
    /// A folder the person pointed at.
    Pointed,
}

/// An engine that is here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Which engine.
    pub engine: EngineKind,
    /// Where it came from.
    pub place: Place,
    /// Its program.
    pub program: PathBuf,
    /// Its release, when the install says it.
    pub version: Option<String>,
}

/// Looks for engines at the fixed system paths and in the two install roots.
#[derive(Debug, Clone)]
pub struct Finder {
    roots: Roots,
    manifest: Manifest,
    packages: Vec<(EngineKind, PathBuf)>,
    held: fn(&Path) -> bool,
}

impl Finder {
    /// A finder over `roots` that trusts a system path when `held` says only root could have
    /// written it.
    pub fn new(roots: Roots, held: fn(&Path) -> bool) -> Self {
        let packages = ENGINES
            .into_iter()
            .flat_map(|kind| {
                engine(kind)
                    .package_paths()
                    .iter()
                    .map(move |path| (kind, PathBuf::from(path)))
            })
            .collect();
        Self {
            roots,
            manifest: Manifest::pinned(),
            packages,
            held,
        }
    }

    /// The same finder over another manifest.
    #[must_use]
    pub fn with_manifest(self, manifest: Manifest) -> Self {
        Self { manifest, ..self }
    }

    /// The same finder over another manifest and other package paths (a test's own).
    #[must_use]
    pub fn with(self, manifest: Manifest, packages: Vec<(EngineKind, PathBuf)>) -> Self {
        Self {
            manifest,
            packages,
            ..self
        }
    }

    /// The program of `engine` that is here: a package's first, then an install for everyone,
    /// then one for this person.
    pub fn find(&self, engine: EngineKind) -> Option<Found> {
        self.package(engine)
            .or_else(|| self.installed(engine, Scope::AllUsers))
            .or_else(|| self.installed(engine, Scope::JustMe))
    }

    fn package(&self, engine: EngineKind) -> Option<Found> {
        self.packages
            .iter()
            .filter(|(owner, _)| *owner == engine)
            .map(|(_, path)| path)
            .find(|path| is_program(path) && (self.held)(path))
            .map(|path| Found {
                engine,
                place: Place::Package,
                program: path.clone(),
                version: None,
            })
    }

    /// An install of ours under `scope`: the newest directory with a record whose program is
    /// there (and, for everyone's, held by root).
    fn installed(&self, engine: EngineKind, scope: Scope) -> Option<Found> {
        let base = self.roots.engine_dir(scope, engine);
        let mut names: Vec<String> = std::fs::read_dir(&base)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort_unstable_by(|a, b| b.cmp(a));
        names.into_iter().find_map(|name| {
            let row = self.manifest.row_in_dir(engine, &name)?;
            let dir = InstallDir::at(base.join(&name));
            let record = Record::read(&dir.record()).ok()?;
            let program = dir.program(row);
            let trusted = match scope {
                Scope::AllUsers => is_program(&program) && (self.held)(&program),
                Scope::JustMe => is_program(&program),
            };
            trusted.then_some(Found {
                engine,
                place: match scope {
                    Scope::AllUsers => Place::System,
                    Scope::JustMe => Place::Person,
                },
                program,
                version: Some(record.version),
            })
        })
    }
}

/// A regular file with an execute bit.
fn is_program(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// An engine run from a folder the person points at (Strata): it counts when the folder holds
/// the engine's marker file and its own program (`Engine::pointed` says where). The engine's
/// release is read from the folder's build record when there is one. `None` for an engine that
/// is not run from a folder.
pub fn pointed(engine: EngineKind, folder: &Path) -> Option<Found> {
    let layout = engine_spec::engine(engine).pointed()?;
    let program = folder.join(layout.program);
    let ready = folder.join(layout.marker).is_file() && is_program(&program);
    ready.then(|| Found {
        engine,
        place: Place::Pointed,
        program,
        version: std::fs::read(folder.join(layout.build_record))
            .ok()
            .and_then(|text| serde_json::from_slice::<serde_json::Value>(&text).ok())
            .and_then(|json| json.get("version")?.as_str().map(str::to_owned)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_manifest::{Arch, Backend, Delivery, Row};

    static ROWS: [Row; 1] = [Row {
        engine: EngineKind::LlamaServer,
        arch: Arch::X86_64,
        backend: Backend::Cpu,
        version: "b1",
        min_nvidia_driver: None,
        delivery: Delivery::Archives(&[]),
        program: "llama-server",
        tree_seal: None,
    }];

    fn program(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }

    fn install(roots: &Roots, scope: Scope, name: &str) {
        let dir = roots.engine_dir(scope, EngineKind::LlamaServer).join(name);
        program(&dir.join("engine/llama-server"));
        let record = Record {
            engine: "llama_cpp".into(),
            version: "b1".into(),
            backend: "cpu".into(),
            seal: String::new(),
            entries: Default::default(),
            venv_seal: None,
        };
        record.write(&dir.join("installed.json")).expect("record");
    }

    fn all_held(_: &Path) -> bool {
        true
    }

    fn finder(roots: &Roots) -> Finder {
        Finder::new(roots.clone(), all_held).with(Manifest::of(&ROWS), Vec::new())
    }

    #[test]
    fn an_install_of_ours_is_found_for_everyone_before_the_person_and_not_while_partial() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let roots = Roots::new(dir.path().join("system"), dir.path().join("person"));
        assert_eq!(finder(&roots).find(EngineKind::LlamaServer), None);
        install(&roots, Scope::JustMe, "b1-cpu");
        install(&roots, Scope::AllUsers, ".b1-cpu.partial");
        let found = finder(&roots).find(EngineKind::LlamaServer).expect("found");
        assert_eq!(
            (found.place, found.version.as_deref()),
            (Place::Person, Some("b1"))
        );
        install(&roots, Scope::AllUsers, "b1-cpu");
        let found = finder(&roots).find(EngineKind::LlamaServer).expect("found");
        assert_eq!(found.place, Place::System);
        assert_eq!(
            found.program,
            roots
                .root(Scope::AllUsers)
                .join("llama_cpp/b1-cpu/engine/llama-server")
        );
    }

    #[test]
    fn a_system_install_root_does_not_hold_is_not_found_and_a_directory_with_no_row_is_ignored() {
        fn nothing_held(_: &Path) -> bool {
            false
        }
        let dir = tempfile::tempdir().expect("scratch dir");
        let roots = Roots::new(dir.path().join("system"), dir.path().join("person"));
        install(&roots, Scope::AllUsers, "b1-cpu");
        install(&roots, Scope::JustMe, "b0-cpu");
        let strict = Finder::new(roots, nothing_held).with(Manifest::of(&ROWS), Vec::new());
        assert_eq!(strict.find(EngineKind::LlamaServer), None);
    }

    #[test]
    fn a_package_program_is_found_first_and_one_that_is_not_there_is_passed_over() {
        let dir = tempfile::tempdir().expect("scratch dir");
        let roots = Roots::new(dir.path().join("system"), dir.path().join("person"));
        install(&roots, Scope::JustMe, "b1-cpu");
        let packaged = dir.path().join("bin/llama-server");
        program(&packaged);
        let at = |paths: Vec<PathBuf>| {
            Finder::new(roots.clone(), all_held)
                .with(
                    Manifest::of(&ROWS),
                    paths
                        .into_iter()
                        .map(|p| (EngineKind::LlamaServer, p))
                        .collect(),
                )
                .find(EngineKind::LlamaServer)
        };
        let gone = dir.path().join("gone");
        assert_eq!(at(vec![gone.clone()]).map(|f| f.place), Some(Place::Person));
        let found = at(vec![gone, packaged.clone()]).expect("found");
        assert_eq!((found.place, found.program), (Place::Package, packaged));
    }

    #[test]
    fn a_strata_folder_counts_when_it_has_its_server_and_python() {
        let dir = tempfile::tempdir().expect("scratch dir");
        assert_eq!(pointed(EngineKind::Strata, dir.path()), None);
        program(&dir.path().join(".venv/bin/python"));
        assert_eq!(pointed(EngineKind::Strata, dir.path()), None);
        std::fs::create_dir_all(dir.path().join("serve")).expect("dir");
        std::fs::write(dir.path().join("serve/server.py"), "").expect("server");
        std::fs::create_dir_all(dir.path().join("engine")).expect("dir");
        std::fs::write(
            dir.path().join("engine/BUILD.json"),
            "{\"version\":\"0.1.42\"}",
        )
        .expect("build");
        let found = pointed(EngineKind::Strata, dir.path()).expect("found");
        assert_eq!(
            (found.place, found.version.as_deref()),
            (Place::Pointed, Some("0.1.42"))
        );
    }
}
