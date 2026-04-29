//! Build script: discover OpenAPI Generator fixtures and generate one `#[test]` per file.
//! Emits `generated_fixture_tests.rs` into OUT_DIR for inclusion under `#[cfg(test)]`.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURES_SUBDIR: &str = "tests/openapi-generator-fixtures";
const OAS30_PREFIX: &str = "oas30/";
const OAS31_PREFIX: &str = "oas31/";
const OAS32_PREFIX: &str = "oas32/";
const SUPPORTED_EXTENSIONS: &[&str] = &["yaml", "yml", "json"];
const SKIP_SUFFIX: &str = "tags.json";
const TEST_FN_PREFIX: &str = "test_oag_fixture_";

/// Load skip paths from `test_fixture_skip_paths.txt` (one path per line; # and blank lines ignored).
fn load_skip_paths(manifest_dir: &Path) -> Vec<String> {
    let path = manifest_dir.join("test_fixture_skip_paths.txt");
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
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
    if !dir.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy();
            collect_fixture_paths(
                &path,
                base,
                &format!("{}{}/", prefix, name),
                skip_paths,
                out,
            );
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

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest_dir = PathBuf::from(&manifest_dir);
    let fixtures_base = manifest_dir.join(FIXTURES_SUBDIR);

    println!("cargo::rerun-if-changed={}", fixtures_base.display());
    println!(
        "cargo::rerun-if-changed={}",
        manifest_dir.join("test_fixture_skip_paths.txt").display()
    );

    let skip_paths = load_skip_paths(&manifest_dir);

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

    let mut test_names = HashSet::new();
    let lines: Vec<String> = rel_paths
        .iter()
        .map(|rel| {
            let base_name = format!("{}{}", TEST_FN_PREFIX, path_to_test_name(rel));
            let unique_name = unique_test_name(&base_name, &mut test_names);
            let rel_escaped = rel.replace('\\', "/").replace('"', r#"\""#);
            format!(
                r#"#[test]
fn {}() {{ super::run_fixture_test("{}"); }}"#,
                unique_name, rel_escaped
            )
        })
        .collect();

    let content = if lines.is_empty() {
        r#"// No fixtures found (run scripts/sync-openapi-generator-fixtures.sh).
#[test]
fn test_oag_fixture_no_fixtures_placeholder() {}"#
            .to_string()
    } else {
        lines.join("\n\n")
    };

    let out_path = Path::new(&out_dir).join("generated_fixture_tests.rs");
    fs::write(out_path, content).expect("write generated_fixture_tests.rs");
}
