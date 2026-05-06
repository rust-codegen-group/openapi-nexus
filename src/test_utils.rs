use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use similar::TextDiff;

use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::FileWriter;

pub fn read_fixture(fixture_path: &str) -> String {
    let possible_paths = [
        Path::new("tests/fixtures").join(fixture_path),
        Path::new("../../../tests/fixtures").join(fixture_path),
        Path::new("../../tests/fixtures").join(fixture_path),
        Path::new("../tests/fixtures").join(fixture_path),
    ];

    for path in &possible_paths {
        if path.exists() {
            return fs::read_to_string(path).unwrap();
        }
    }
    panic!("Could not find fixture: {}", fixture_path);
}

fn to_golden_filename(filename: &str) -> String {
    format!("{}.golden", filename)
}

fn from_golden_filename(golden_filename: &str) -> Option<String> {
    golden_filename
        .strip_suffix(".golden")
        .map(|s| s.to_string())
}

pub fn generate_files<G: CodeGenerator + FileWriter>(
    generator: &G,
    spec_content: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let parsed = crate::parser::parse_content_yaml(spec_content)?;
    let ir = crate::ir::lower::lower(parsed)?;
    let generated_files = generator.generate(&ir)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_dir = env::temp_dir().join(format!(
        "openapi_nexus_test_{}_{}_{}",
        std::process::id(),
        timestamp,
        seq,
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    generator.write_files(&temp_dir, &generated_files)?;

    let mut result = HashMap::new();
    read_directory_recursive(&temp_dir, &temp_dir, &mut result);
    fs::remove_dir_all(&temp_dir).unwrap();

    Ok(result)
}

fn read_directory_recursive(
    base_dir: &Path,
    current_dir: &Path,
    result: &mut HashMap<String, String>,
) {
    for entry in fs::read_dir(current_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() {
            read_directory_recursive(base_dir, &path, result);
        } else if path.is_file() {
            let relative_path = path.strip_prefix(base_dir).unwrap();
            let filename = relative_path.to_string_lossy().replace('\\', "/");
            let content = fs::read_to_string(&path).unwrap();
            result.insert(filename, content);
        }
    }
}

pub fn run_golden_test<G: CodeGenerator + FileWriter>(
    generator: &G,
    golden_dir: &Path,
    spec_name: &str,
    fixture_path: &str,
    update_hint: &str,
) {
    let spec_content = read_fixture(fixture_path);
    let generated = generate_files(generator, &spec_content).unwrap_or_else(|e| {
        panic!("Failed to generate files for {}: {}", spec_name, e);
    });

    if env::var("UPDATE_GOLDEN").is_ok() {
        update_golden_files(golden_dir, spec_name, &generated);
    } else {
        compare_with_golden_files(golden_dir, spec_name, &generated, update_hint);
    }
}

fn update_golden_files(golden_dir: &Path, spec_name: &str, generated: &HashMap<String, String>) {
    let spec_dir = golden_dir.join(spec_name);

    if spec_dir.exists() {
        fs::remove_dir_all(&spec_dir).unwrap();
    }
    fs::create_dir_all(&spec_dir).unwrap();

    for (filename, content) in generated {
        let golden_filename = to_golden_filename(filename);
        let file_path = spec_dir.join(&golden_filename);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        fs::write(&file_path, content).unwrap();
        println!("Updated: {}", file_path.display());
    }
    println!("Updated golden files for {}", spec_name);
}

fn compare_with_golden_files(
    golden_dir: &Path,
    spec_name: &str,
    generated: &HashMap<String, String>,
    update_hint: &str,
) {
    let spec_dir = golden_dir.join(spec_name);
    let mut golden_filenames = HashSet::new();
    compare_directories_recursive(
        &spec_dir,
        &spec_dir,
        generated,
        spec_name,
        update_hint,
        &mut golden_filenames,
    );

    let extra: Vec<&String> = generated
        .keys()
        .filter(|k| !golden_filenames.contains(k.as_str()))
        .collect();
    if !extra.is_empty() {
        let mut sorted = extra.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        sorted.sort();
        panic!(
            "Generated files not present in golden directory for {}:\n  {}\n\nTo update golden files, run:\n   {}",
            spec_name,
            sorted.join("\n  "),
            update_hint,
        );
    }

    println!("Golden file test passed for {}", spec_name);
}

fn compare_directories_recursive(
    base_dir: &Path,
    current_dir: &Path,
    generated: &HashMap<String, String>,
    spec_name: &str,
    update_hint: &str,
    golden_filenames: &mut HashSet<String>,
) {
    assert!(
        current_dir.exists(),
        "Golden directory not found: {}",
        current_dir.display()
    );

    for entry in fs::read_dir(current_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() {
            compare_directories_recursive(
                base_dir,
                &path,
                generated,
                spec_name,
                update_hint,
                golden_filenames,
            );
        } else if path.is_file() {
            let relative_path = path.strip_prefix(base_dir).unwrap();
            let golden_filename = relative_path.to_string_lossy().replace('\\', "/");

            let filename = from_golden_filename(&golden_filename).unwrap_or_else(|| {
                panic!(
                    "Golden file does not have .golden suffix: {}",
                    golden_filename
                );
            });

            golden_filenames.insert(filename.clone());

            let generated_content = generated.get(&filename).unwrap_or_else(|| {
                panic!(
                    "Generated file not found for golden file: {}",
                    golden_filename
                );
            });

            let golden_content = fs::read_to_string(&path).unwrap();

            if generated_content != &golden_content {
                let diff = TextDiff::from_lines(&golden_content, generated_content);
                let unified = diff
                    .unified_diff()
                    .context_radius(3)
                    .header("golden", "generated")
                    .to_string();
                panic!(
                    "Content mismatch in: {}/{}\n{}\n{}\nTo update golden files, run:\n   {}",
                    spec_name,
                    filename,
                    "=".repeat(80),
                    unified,
                    update_hint,
                );
            }
        }
    }
}

/// Helper to build a test case list from a slice of `(name, fixture_path)` pairs.
pub fn test_cases_from_slice(
    cases: &[(&'static str, &'static str)],
) -> HashMap<&'static str, &'static str> {
    cases.iter().copied().collect()
}
