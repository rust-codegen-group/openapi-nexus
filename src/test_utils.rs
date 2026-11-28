use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use crate::codegen::artifact_manifest::plan_output;
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::FileWriter;

mod golden_compare;
mod golden_update;

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

pub fn generate_files<G: CodeGenerator + FileWriter>(
    generator: &G,
    spec_content: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let parsed = crate::parser::parse_content_yaml(spec_content)?;
    let ir = crate::ir::lower::lower(parsed)?;
    let generated_files = generator.generate(&ir)?;

    Ok(plan_output(generator.source_dir(), &generated_files)?
        .into_iter()
        .map(|(path, content)| (path, content.to_owned()))
        .collect())
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

    if golden_update::requested(env::var_os("UPDATE_GOLDEN").as_deref()) {
        update_golden_files(golden_dir, spec_name, &generated);
    } else {
        compare_with_golden_files(golden_dir, spec_name, &generated, update_hint);
    }
}

/// Compare a selected (.golden) file, or safely refresh it in update mode.
pub fn run_golden_file_test(path: &Path, generated: &str) {
    if golden_update::requested(env::var_os("UPDATE_GOLDEN").as_deref()) {
        golden_update::replace_file(path, generated)
            .unwrap_or_else(|error| panic!("Could not update {}: {error}", path.display()));
    } else {
        let expected = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("Could not read {}: {error}", path.display()));
        assert_eq!(generated, expected, "{}", path.display());
    }
}

fn update_golden_files(golden_dir: &Path, spec_name: &str, generated: &HashMap<String, String>) {
    golden_update::replace_tree(golden_dir, spec_name, generated).unwrap_or_else(|error| {
        panic!(
            "Could not update {}: {error}",
            golden_dir.join(spec_name).display()
        )
    });
    println!("Updated golden files for {spec_name}");
}

fn compare_with_golden_files(
    golden_dir: &Path,
    spec_name: &str,
    generated: &HashMap<String, String>,
    update_hint: &str,
) {
    let spec_dir = golden_dir.join(spec_name);
    match golden_compare::compare(&spec_dir, generated) {
        Ok(Some(report)) => panic!(
            "Golden snapshots differ for {spec_name}:\n{report}\nTo update golden files, run:\n   {update_hint}"
        ),
        Ok(None) => println!("Golden file test passed for {spec_name}"),
        Err(error) => panic!("Could not compare golden snapshots for {spec_name}: {error}"),
    }
}

#[cfg(test)]
mod golden_update_tests;
