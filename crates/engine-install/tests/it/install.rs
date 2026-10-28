use crate::host::FileHost;
use crate::support::{fetcher, fixture, roots, row};
use engine_install::{CancelToken, InstallError, Installer, Meter};
use engine_manifest::{Failure, Manifest};
use engine_probe::{Finder, Place, Record, Scope, Verdict, Verifier, seal_of};
use model_catalog::EngineKind;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn held(_: &Path) -> bool {
    true
}

#[tokio::test]
async fn an_archive_installs_into_the_persons_directory_and_is_found_and_verified_there() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let host = FileHost::start().await;
    let release = fixture(&dir, "a", "echo hi");
    host.put("/e.tar.gz", release.archive.clone());
    let row = row(
        &host.url("/e.tar.gz"),
        &release,
        &release.seal,
        release.archive.len() as u64,
    );
    let roots = roots(dir.path());
    let installer = Installer::new(fetcher(host.roots()), roots.clone());
    let meter = Meter::new(row.download_size());
    let installed = installer
        .install(Scope::JustMe, row, &meter, &CancelToken::new())
        .await
        .expect("installed");
    let program = installed.program(row);
    let mode = std::fs::metadata(&program)
        .expect("program")
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0, "the program runs");
    assert_eq!(meter.permille(), 1000);
    assert_eq!(
        Record::read(&installed.record()).expect("record").seal,
        release.seal
    );
    assert!(
        !installed.partial().exists(),
        "the partial directory is gone"
    );
    let manifest = Manifest::of(Box::leak(vec![*row].into_boxed_slice()));
    let found = Finder::new(roots.clone(), held)
        .with(manifest, Vec::new())
        .find(EngineKind::LlamaServer)
        .expect("found");
    assert_eq!(
        (found.place, found.program.clone()),
        (Place::Person, program.clone())
    );
    let verifier = Verifier::new(roots, manifest);
    assert_eq!(verifier.check(&program), Verdict::Verified);
    std::fs::write(program.with_file_name("planted.so"), "evil").expect("plant");
    assert_eq!(
        verifier.check(&program),
        Verdict::Changed(EngineKind::LlamaServer)
    );
    // A second install of what is there asks for nothing.
    let asked = host.hits().len();
    let again = installer
        .install(Scope::JustMe, row, &Meter::new(1), &CancelToken::new())
        .await;
    assert!(again.is_ok());
    assert_eq!(host.hits().len(), asked);
}

#[tokio::test]
async fn files_that_are_not_the_pinned_tree_are_not_installed_and_leave_nothing() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let host = FileHost::start().await;
    let release = fixture(&dir, "b", "echo hi");
    host.put("/e.tar.gz", release.archive.clone());
    let wrong = "1".repeat(64);
    let row = row(
        &host.url("/e.tar.gz"),
        &release,
        &wrong,
        release.archive.len() as u64,
    );
    let roots = roots(dir.path());
    let outcome = Installer::new(fetcher(host.roots()), roots.clone())
        .install(Scope::JustMe, row, &Meter::new(1), &CancelToken::new())
        .await;
    let error = outcome.expect_err("refused");
    assert_eq!(error, InstallError::SealMismatch);
    assert_eq!(error.failure(), Failure::Corrupt);
    let leftovers = roots.engine_dir(Scope::JustMe, EngineKind::LlamaServer);
    assert_eq!(
        std::fs::read_dir(leftovers)
            .map(Iterator::count)
            .unwrap_or(0),
        0
    );
}

#[tokio::test]
async fn too_little_room_stops_before_anything_is_downloaded() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let host = FileHost::start().await;
    let release = fixture(&dir, "c", "echo hi");
    host.put("/e.tar.gz", release.archive.clone());
    let row = row(
        &host.url("/e.tar.gz"),
        &release,
        &release.seal,
        u64::MAX / 4,
    );
    let outcome = Installer::new(fetcher(host.roots()), roots(dir.path()))
        .install(Scope::JustMe, row, &Meter::new(1), &CancelToken::new())
        .await;
    let error = outcome.expect_err("no room");
    assert!(matches!(error, InstallError::Space { .. }));
    assert_eq!(error.failure(), Failure::NoSpace);
    assert!(host.hits().is_empty());
}

#[tokio::test]
async fn a_stop_leaves_no_install_and_the_fixture_seal_is_the_unpacked_trees() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let host = FileHost::start().await;
    let release = fixture(&dir, "d", "echo hi");
    host.put("/e.tar.gz", release.archive.clone());
    let row = row(
        &host.url("/e.tar.gz"),
        &release,
        &release.seal,
        release.archive.len() as u64,
    );
    let roots = roots(dir.path());
    let stop = CancelToken::new();
    stop.cancel();
    let outcome = Installer::new(fetcher(host.roots()), roots.clone())
        .install(Scope::JustMe, row, &Meter::new(1), &stop)
        .await;
    assert_eq!(outcome.expect_err("stopped").failure(), Failure::Cancelled);
    assert!(!roots.dir(Scope::JustMe, row).path().exists());
    assert_eq!(
        seal_of(&dir.path().join("fixture-d/top"))
            .expect("seal")
            .as_hex(),
        release.seal
    );
}
