//! Golden file tests for TypeScript code generation
//!
//! These tests compare generated TypeScript code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use similar::TextDiff;
use tracing_test::traced_test;
use utoipa::openapi::OpenApi;

use openapi_nexus_core::traits::code_generator::LanguageCodeGenerator as _;
use openapi_nexus_core::traits::file_writer::FileWriter;
use openapi_nexus_typescript::TsLangGenerator;
use openapi_nexus_typescript::config::TsConfig;

/// Read a fixture file from various possible locations
fn read_fixture(fixture_path: &str) -> String {
    let possible_paths = [
        Path::new("tests/fixtures").join(fixture_path),
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

/// Get the golden directory path
fn get_golden_dir() -> &'static Path {
    Path::new("../tests/golden/typescript")
}

/// Generate TypeScript files from an OpenAPI specification
fn generate_typescript_files(
    spec_content: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let openapi: OpenApi = serde_norway::from_str(spec_content)?;
    let config = TsConfig::default();
    let generator = TsLangGenerator::new(config);
    let generated_files = match generator.generate(&openapi) {
        Ok(files) => {
            println!("Successfully generated {} files", files.len());
            files
        }
        Err(e) => {
            println!("Error generating files: {}", e);
            return Err(e);
        }
    };

    // Create a unique temporary directory to write files with proper directory structure
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "openapi_nexus_test_{}_{}",
        std::process::id(),
        timestamp
    ));
    fs::create_dir_all(&temp_dir).unwrap();

    // Use the FileWriter trait to write files with proper directory organization
    if let Err(e) = generator.write_files(&temp_dir, &generated_files) {
        println!("Error writing files: {}", e);
        println!("Temp directory: {}", temp_dir.display());
        println!("Generated files count: {}", generated_files.len());
        for (i, file) in generated_files.iter().enumerate() {
            println!(
                "  File {}: {} (category: {:?})",
                i, file.filename, file.category
            );
        }
        return Err(e);
    }

    // Read all files recursively from the temporary directory
    let mut result = HashMap::new();
    read_directory_recursive(&temp_dir, &temp_dir, &mut result);

    // Clean up temporary directory
    fs::remove_dir_all(&temp_dir).unwrap();

    Ok(result)
}

/// Recursively read all files from a directory
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
            // Normalize path separators to forward slashes for consistency
            let filename = relative_path.to_string_lossy().replace('\\', "/");
            let content = fs::read_to_string(&path).unwrap();
            result.insert(filename, content);
        }
    }
}

/// Update or compare golden files for a given spec
fn test_golden_files(
    spec_name: &str,
    fixture_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spec_content = read_fixture(fixture_path);
    let generated = match generate_typescript_files(&spec_content) {
        Ok(files) => files,
        Err(e) => {
            println!(
                "Failed to generate TypeScript files for {}: {}",
                spec_name, e
            );
            return Err(e);
        }
    };
    let update_mode = env::var("UPDATE_GOLDEN").is_ok();

    if update_mode {
        update_golden_files(spec_name, &generated)?;
    } else {
        compare_with_golden_files(spec_name, &generated)?;
    }

    Ok(())
}

/// Update golden files with generated content
fn update_golden_files(
    spec_name: &str,
    generated: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!(
        "UPDATE_GOLDEN mode: updating golden files for {}",
        spec_name
    );
    let golden_dir = get_golden_dir().join(spec_name);

    // Clean up existing files before updating
    if golden_dir.exists() {
        println!("Cleaning up existing files in: {}", golden_dir.display());
        fs::remove_dir_all(&golden_dir)?;
    }

    fs::create_dir_all(&golden_dir)?;

    for (filename, content) in generated {
        let file_path = golden_dir.join(filename);

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&file_path, content)?;
        println!("Updated: {}", file_path.display());
    }
    println!("Updated golden files for {}", spec_name);
    Ok(())
}

/// Compare generated files with golden files and report differences
fn compare_with_golden_files(
    spec_name: &str,
    generated: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let golden_dir = get_golden_dir().join(spec_name);

    // Recursively compare directories
    compare_directories_recursive(&golden_dir, &golden_dir, generated, spec_name)?;

    println!("Golden file test passed for {}", spec_name);
    Ok(())
}

/// Recursively compare directories and files
fn compare_directories_recursive(
    base_dir: &Path,
    current_dir: &Path,
    generated: &HashMap<String, String>,
    spec_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !current_dir.exists() {
        println!("Golden directory not found: {}", current_dir.display());
        return Err(format!("Golden directory not found: {}", current_dir.display()).into());
    }

    // Walk through the golden directory recursively
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively compare subdirectories
            compare_directories_recursive(base_dir, &path, generated, spec_name)?;
        } else if path.is_file() {
            // Compare individual files
            let relative_path = path.strip_prefix(base_dir)?;
            // Normalize path separators to forward slashes for consistency
            let filename = relative_path.to_string_lossy().replace('\\', "/");

            if let Some(generated_content) = generated.get(&filename) {
                let golden_content = fs::read_to_string(&path)?;

                if generated_content != &golden_content {
                    show_diff(spec_name, &filename, &golden_content, generated_content);
                    return Err(
                        format!("Golden file mismatch for {}: {}", spec_name, filename).into(),
                    );
                }
            } else {
                println!("Generated file not found for golden file: {}", filename);
                return Err(
                    format!("Generated file not found for golden file: {}", filename).into(),
                );
            }
        }
    }

    Ok(())
}

/// Show a diff when golden files don't match
fn show_diff(spec_name: &str, filename: &str, golden: &str, generated: &str) {
    println!("Content mismatch in: {}/{}", spec_name, filename);
    println!("{}", "=".repeat(80));

    let diff = TextDiff::from_lines(golden, generated);
    println!(
        "{}",
        diff.unified_diff()
            .context_radius(3)
            .header("golden", "generated")
    );

    println!("To update golden files, run:");
    println!("   UPDATE_GOLDEN=1 cargo test --test golden_tests");
}

#[test]
#[traced_test]
fn test_petstore_golden() {
    test_golden_files("petstore", "valid/petstore.yaml").unwrap();
}

#[test]
#[traced_test]
fn test_minimal_golden() {
    if let Err(e) = test_golden_files("minimal", "valid/minimal.yaml") {
        println!("Minimal golden test failed: {}", e);
        process::exit(1);
    }
}

#[test]
#[traced_test]
fn test_comprehensive_schemas_golden() {
    if let Err(e) = test_golden_files("comprehensive-schemas", "valid/comprehensive-schemas.yaml") {
        println!("Comprehensive schemas golden test failed: {}", e);
        process::exit(1);
    }
}

#[test]
#[traced_test]
fn test_server_object_golden() {
    if let Err(e) = test_golden_files("server-object", "valid/server-object.yaml") {
        println!("Server object golden test failed: {}", e);
        process::exit(1);
    }
}

#[test]
#[traced_test]
fn test_runtime_generation() {
    let spec_content = read_fixture("valid/minimal.yaml");
    let openapi: OpenApi = serde_norway::from_str(&spec_content).unwrap();

    let config = openapi_nexus_typescript::config::TsConfig::default();
    let generator = TsLangGenerator::new(config);
    let generated_files = generator.generate(&openapi).unwrap();

    // Find the runtime file
    let runtime_file = generated_files
        .iter()
        .find(|file| file.filename == "runtime.ts")
        .expect("Runtime file should be generated");

    // Verify the runtime file contains expected content
    assert!(runtime_file.content.contains("export const BASE_PATH"));
    assert!(runtime_file.content.contains("export class Configuration"));
    assert!(runtime_file.content.contains("export class BaseAPI"));
    assert!(runtime_file.content.contains("export class ResponseError"));
    assert!(runtime_file.content.contains("export class FetchError"));
    assert!(runtime_file.content.contains("export class RequiredError"));
    assert!(
        runtime_file
            .content
            .contains("export const COLLECTION_FORMATS")
    );
    assert!(runtime_file.content.contains("export type FetchAPI"));
    assert!(
        runtime_file
            .content
            .contains("export interface ConfigurationParameters")
    );
    assert!(runtime_file.content.contains("export interface Middleware"));
    assert!(runtime_file.content.contains("export function querystring"));
    assert!(
        runtime_file
            .content
            .contains("export class JSONApiResponse")
    );
    assert!(
        runtime_file
            .content
            .contains("export class VoidApiResponse")
    );
    assert!(
        runtime_file
            .content
            .contains("export class BlobApiResponse")
    );
    assert!(
        runtime_file
            .content
            .contains("export class TextApiResponse")
    );

    // Verify it has the do_not_edit header
    assert!(
        runtime_file
            .content
            .contains("Do not edit the file manually")
    );

    // Verify it uses the correct base path from the OpenAPI spec (default since no servers specified)
    assert!(runtime_file.content.contains("http://localhost"));

    println!(
        "Runtime file content length: {}",
        runtime_file.content.len()
    );
    println!("Runtime file generated successfully!");
}
