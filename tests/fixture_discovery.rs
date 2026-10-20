//! Exercise the actual build-support logic in-process, without compiling a harness.

use std::{fs, path::Path};

#[path = "../build_support/fixture_tests.rs"]
mod fixture_tests;

#[test]
fn nested_fixture_paths_resolve_once_under_the_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let registrations = fixture_tests::generate_fixture_tests(root);
    assert!(registrations.contains("super::run_fixture_test(\"oas30/ada/petstore.yaml\")"));

    let paths: Vec<_> = registrations
        .lines()
        .filter_map(|line| line.split_once("super::run_fixture_test(\""))
        .map(|(_, rest)| rest.split_once('"').expect("registered path ends").0)
        .collect();
    assert!(!paths.is_empty(), "the corpus must register real fixtures");
    for relative in paths {
        let path = root.join("tests/openapi-generator-fixtures").join(relative);
        fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("registered fixture {}: {error}", path.display()));
    }
}

#[test]
fn empty_discovery_registers_a_failure_instead_of_a_passing_placeholder() {
    // This checked-in directory has no upstream corpus or expectation manifests.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid");
    assert!(root.is_dir());
    assert_eq!(
        fixture_tests::generate_fixture_tests(&root),
        r#"// No fixtures found (run scripts/sync-openapi-generator-fixtures.sh).
#[test]
fn test_oag_fixture_requires_nonempty_selection() {
    panic!("no OpenAPI fixtures discovered; run scripts/sync-openapi-generator-fixtures.sh");
}"#
    );
}

#[test]
#[should_panic(expected = "rejection fixture not registered: oas30/missing.yaml")]
fn missing_rejection_fixture_fails_even_when_other_fixtures_are_registered() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/discovery/missing-rejection");
    fixture_tests::generate_fixture_tests(&root);
}

#[test]
#[should_panic(expected = "rejection fixture not registered: oas30/rejected.yaml")]
fn excluded_rejection_fixture_fails_even_when_its_file_exists() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/discovery/excluded-rejection");
    assert!(
        root.join("tests/openapi-generator-fixtures/oas30/rejected.yaml")
            .is_file()
    );
    fixture_tests::generate_fixture_tests(&root);
}
