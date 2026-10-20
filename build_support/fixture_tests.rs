//! Shared fixture discovery and Cargo test registration used by the build script.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

const FIXTURES_SUBDIR: &str = "tests/openapi-generator-fixtures";
const OAS30_PREFIX: &str = "oas30/";
const OAS31_PREFIX: &str = "oas31/";
const OAS32_PREFIX: &str = "oas32/";
const SUPPORTED_EXTENSIONS: &[&str] = &["yaml", "yml", "json"];
const SKIP_SUFFIX: &str = "tags.json";
const TEST_FN_PREFIX: &str = "test_oag_fixture_";

/// Load fixture expectation paths (one per line; # and blank lines ignored).
fn load_paths(manifest_dir: &Path, filename: &str) -> Vec<String> {
    let path = manifest_dir.join(filename);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot read {}: {error}", path.display()),
    };
    content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect()
}

fn is_skip_path(rel: &str, skip_paths: &[String]) -> bool {
    if rel.ends_with(SKIP_SUFFIX) {
        return true;
    }
    skip_paths.iter().any(|s| s == rel)
}

fn collect_fixture_paths(
    dir: &Path,
    base: &Path,
    prefix: &str,
    skip_paths: &[String],
    out: &mut Vec<String>,
) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot discover fixtures in {}: {error}", dir.display()));
    for e in entries {
        let e = e.expect("fixture directory entry is readable");
        let path = e.path();
        if path.is_dir() {
            collect_fixture_paths(&path, base, prefix, skip_paths, out);
        } else if path.is_file() {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let full_path = format!("{}{}", prefix, rel_str);
            let ext_ok = rel
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| SUPPORTED_EXTENSIONS.contains(&e))
                .unwrap_or(false);
            if ext_ok && !is_skip_path(&full_path, skip_paths) {
                out.push(full_path);
            }
        }
    }
}

/// Sanitize relative path to a valid Rust test function name (alphanumeric and `_` only).
fn path_to_test_name(rel: &str) -> String {
    let s: String = rel
        .chars()
        .map(|c| match c {
            '/' | '.' | '-' => '_',
            c if c.is_ascii_alphanumeric() || c == '_' => c,
            _ => '_',
        })
        .collect();
    s.trim_matches('_')
        .split("__")
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn unique_test_name(base_name: &str, taken: &mut HashSet<String>) -> String {
    let mut name = base_name.to_string();
    let mut counter = 0u32;
    while !taken.insert(name.clone()) {
        counter += 1;
        name = format!("{}_{}", base_name, counter);
    }
    name
}

/// Discover the corpus and emit registrations for Cargo's native test runner.
pub fn generate_fixture_tests(manifest_dir: &Path) -> String {
    let fixtures_base = manifest_dir.join(FIXTURES_SUBDIR);
    let skip_paths = load_paths(manifest_dir, "test_fixture_skip_paths.txt");
    let rejections = load_paths(manifest_dir, "test_fixture_rejections.txt");

    let oas30 = fixtures_base.join("oas30");
    let oas31 = fixtures_base.join("oas31");
    let oas32 = fixtures_base.join("oas32");
    let mut rel_paths = Vec::new();
    if oas30.exists() {
        collect_fixture_paths(&oas30, &oas30, OAS30_PREFIX, &skip_paths, &mut rel_paths);
    }
    if oas31.exists() {
        collect_fixture_paths(&oas31, &oas31, OAS31_PREFIX, &skip_paths, &mut rel_paths);
    }
    if oas32.exists() {
        collect_fixture_paths(&oas32, &oas32, OAS32_PREFIX, &skip_paths, &mut rel_paths);
    }
    rel_paths.sort();

    for rejection in &rejections {
        assert!(
            rel_paths.binary_search(rejection).is_ok(),
            "rejection fixture not registered: {rejection} (missing or excluded)"
        );
    }

    let mut test_names = HashSet::new();
    let lines: Vec<String> = rel_paths
        .iter()
        .map(|rel| {
            let base_name = format!("{}{}", TEST_FN_PREFIX, path_to_test_name(rel));
            let unique_name = unique_test_name(&base_name, &mut test_names);
            let rel_escaped = rel.replace('\\', "/").replace('"', r#"\""#);
            let expectation = if rejections.contains(rel) {
                "#[should_panic(expected = \"parse oas\")]\n"
            } else {
                ""
            };
            format!(
                r#"#[test]
{}fn {}() {{ super::run_fixture_test("{}"); }}"#,
                expectation, unique_name, rel_escaped
            )
        })
        .collect();

    if lines.is_empty() {
        r#"// No fixtures found (run scripts/sync-openapi-generator-fixtures.sh).
#[test]
fn test_oag_fixture_requires_nonempty_selection() {
    panic!("no OpenAPI fixtures discovered; run scripts/sync-openapi-generator-fixtures.sh");
}"#
        .to_string()
    } else {
        lines.join("\n\n")
    }
}
