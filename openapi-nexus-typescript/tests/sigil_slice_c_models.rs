//! Phase 3 Slice C: prove-out that `sigil_emit::generate_model_files` renders
//! `multiple-similar-request-schemas.yaml` to the shape described in
//! `docs/target-output-spec.md`.
//!
//! Golden test. Comparison-only; set `UPDATE_SIGIL_GOLDEN=1` to refresh.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use openapi_nexus_typescript::sigil_emit;
use similar::TextDiff;

fn read_fixture(rel: &str) -> String {
    for base in ["tests/fixtures", "../tests/fixtures", "../../tests/fixtures"] {
        let p = Path::new(base).join(rel);
        if p.exists() {
            return fs::read_to_string(p).unwrap();
        }
    }
    panic!("fixture not found: {rel}");
}

fn golden_dir() -> PathBuf {
    for base in [
        "tests/golden/typescript/typescript-fetch-sigil",
        "../tests/golden/typescript/typescript-fetch-sigil",
        "../../tests/golden/typescript/typescript-fetch-sigil",
    ] {
        let p = Path::new(base);
        if p.parent().map(|pp| pp.exists()).unwrap_or(false) {
            return p.to_path_buf();
        }
    }
    panic!("golden parent not found");
}

#[test]
fn multiple_similar_request_schemas_models_match_target() {
    let yaml = read_fixture("valid/multiple-similar-request-schemas.yaml");
    let parsed = openapi_nexus_parser::parse_content_yaml(&yaml).unwrap();
    let ir = openapi_nexus_ir::lower::lower(parsed).unwrap();

    let files = sigil_emit::generate_model_files(&ir).expect("slice C renders");
    let generated: HashMap<String, String> = files
        .into_iter()
        .map(|f| (f.filename, f.content))
        .collect();

    let spec_dir = golden_dir().join("multiple-similar-request-schemas/models");

    if env::var("UPDATE_SIGIL_GOLDEN").is_ok() {
        if spec_dir.exists() {
            fs::remove_dir_all(&spec_dir).unwrap();
        }
        fs::create_dir_all(&spec_dir).unwrap();
        for (name, content) in &generated {
            fs::write(spec_dir.join(format!("{name}.golden")), content).unwrap();
        }
        return;
    }

    assert!(
        spec_dir.exists(),
        "golden dir missing: {} — run `UPDATE_SIGIL_GOLDEN=1 cargo test --test sigil_slice_c_delete` first",
        spec_dir.display(),
    );

    // Compare every golden against the matching generated file, and fail if
    // there are extra/missing files either way.
    let mut golden_names: Vec<String> = fs::read_dir(&spec_dir)
        .unwrap()
        .filter_map(|e| {
            e.ok()
                .and_then(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".golden"))
                .map(|n| n.trim_end_matches(".golden").to_string())
        })
        .collect();
    golden_names.sort();
    let mut gen_names: Vec<String> = generated.keys().cloned().collect();
    gen_names.sort();
    assert_eq!(
        gen_names, golden_names,
        "generated file set differs from golden set"
    );

    for name in &gen_names {
        let got = generated.get(name).unwrap();
        let want = fs::read_to_string(spec_dir.join(format!("{name}.golden"))).unwrap();
        if got != &want {
            let diff = TextDiff::from_lines(&want, got);
            eprintln!(
                "{}",
                diff.unified_diff().context_radius(3).header("golden", "generated")
            );
            panic!("sigil golden mismatch for {name}");
        }
    }
}
