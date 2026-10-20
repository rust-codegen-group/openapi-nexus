//! Register upstream fixtures with Cargo's native test runner.

use std::{env, fs, path::PathBuf};

#[path = "build_support/fixture_tests.rs"]
mod fixture_tests;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    for input in [
        "build_support/fixture_tests.rs",
        "tests/openapi-generator-fixtures",
        "test_fixture_skip_paths.txt",
        "test_fixture_rejections.txt",
    ] {
        println!(
            "cargo::rerun-if-changed={}",
            manifest_dir.join(input).display()
        );
    }
    fs::write(
        out_dir.join("generated_fixture_tests.rs"),
        fixture_tests::generate_fixture_tests(&manifest_dir),
    )
    .expect("write generated_fixture_tests.rs");
}
