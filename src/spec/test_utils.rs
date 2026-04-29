//! Test-only utilities for fixture-based spec parsing tests.

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::spec::{OpenApiV30Spec, OpenApiV31Spec};

const FIXTURES_SUBDIR: &str = "tests/openapi-generator-fixtures";
const OAS30_PREFIX: &str = "oas30/";
const OAS31_PREFIX: &str = "oas31/";

/// Candidate base directories for the fixtures tree (order matters: first found wins).
fn fixture_base_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        candidates.push(
            Path::new(&manifest_dir)
                .join("..")
                .join("..")
                .join(FIXTURES_SUBDIR),
        );
        candidates.push(Path::new(&manifest_dir).join("..").join(FIXTURES_SUBDIR));
    }
    candidates.extend([
        Path::new(FIXTURES_SUBDIR).to_path_buf(),
        Path::new("../").join(FIXTURES_SUBDIR),
        Path::new("../../").join(FIXTURES_SUBDIR),
        Path::new("../../../").join(FIXTURES_SUBDIR),
    ]);
    candidates
}

/// Resolve `rel_path` to an absolute path under one of the candidate fixture bases.
/// Returns `None` if the path does not exist under any candidate (e.g. stale generated test).
fn resolve_fixture_path(rel_path: &str) -> Option<PathBuf> {
    fixture_base_candidates().into_iter().find_map(|base| {
        let p = base.join(rel_path);
        if p.exists() { Some(p) } else { None }
    })
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
/// Skips (passes) if the path does not exist under any candidate (stale generated test).
pub fn run_fixture_test(rel_path: &str) {
    let path = match resolve_fixture_path(rel_path) {
        Some(p) => p,
        None => return, // path missing, e.g. stale generated test; skip
    };
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    parse_fixture(&content, ext, rel_path);
}
