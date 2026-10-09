//! The integration tests of `cua-session`, one binary (one link) for all of them.

mod cassette;
mod context;
mod shapes;
mod steps;
mod support;
mod wire_dialects;

/// Fails when a test file is not declared above: a file left out would silently never run.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let declared = include_str!("main.rs");
    for entry in std::fs::read_dir(&tests).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            path.extension().is_none_or(|e| e != "rs"),
            "{} is a binary of its own; move it under tests/it/",
            path.display()
        );
    }
    for entry in std::fs::read_dir(tests.join("it")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let is_module = path.is_dir() || path.extension().is_some_and(|e| e == "rs");
        if is_module && name != "main" {
            assert!(
                declared.contains(&format!("mod {name};")),
                "tests/it/{name} is not declared in tests/it/main.rs"
            );
        }
    }
}
