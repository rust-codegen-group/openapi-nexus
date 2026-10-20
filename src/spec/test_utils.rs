//! Test-only utilities for fixture-based spec parsing tests.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::spec::{OpenApiV30Spec, OpenApiV31Spec};

const FIXTURES_SUBDIR: &str = "tests/openapi-generator-fixtures";
const OAS30_PREFIX: &str = "oas30/";
const OAS31_PREFIX: &str = "oas31/";

/// Resolve a fixture under this checkout, independent of the test's working directory.
fn resolve_fixture_path(rel_path: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES_SUBDIR)
        .join(rel_path);
    path.is_file().then_some(path)
}

/// Parse fixture content as OAS 3.0 or 3.1 spec based on `rel_path` prefix.
fn parse_fixture(content: &str, ext: &str, rel_path: &str) {
    if rel_path.starts_with(OAS30_PREFIX) {
        let _: OpenApiV30Spec = match ext {
            "json" => serde_json::from_str(content).expect("parse oas30 json"),
            "yaml" | "yml" => serde_norway::from_str(content).expect("parse oas30 yaml"),
            _ => panic!("unsupported extension: {}", ext),
        };
    } else if rel_path.starts_with(OAS31_PREFIX) {
        let _: OpenApiV31Spec = match ext {
            "json" => serde_json::from_str(content).expect("parse oas31 json"),
            "yaml" | "yml" => serde_norway::from_str(content).expect("parse oas31 yaml"),
            _ => panic!("unsupported extension: {}", ext),
        };
    } else {
        panic!("rel_path must start with oas30/ or oas31/: {}", rel_path);
    }
}

/// Load fixture at `rel_path`, parse as OAS 3.0 or 3.1, and panic on error.
pub fn run_fixture_test(rel_path: &str) {
    let path =
        resolve_fixture_path(rel_path).unwrap_or_else(|| panic!("fixture not found: {rel_path}"));
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    parse_fixture(&content, ext, rel_path);
}

#[cfg(test)]
mod tests {
    #[test]
    #[should_panic(expected = "fixture not found")]
    fn missing_fixture_is_a_failure_not_a_passing_parser_test() {
        super::run_fixture_test("oas30/__missing_fixture_regression__.yaml");
    }
}
