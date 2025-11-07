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

use openapi_nexus_config::TypeScriptConfig;
use openapi_nexus_core::traits::code_generator::LanguageCodeGenerator as _;
use openapi_nexus_core::traits::file_writer::FileWriter;
use openapi_nexus_typescript::TsLangGenerator;

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
    let config = TypeScriptConfig::default();
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

/// Map of test case names to their fixture paths
#[rustfmt::skip]
fn get_golden_test_cases() -> HashMap<&'static str, &'static str> {
    [
        ("petstore", "valid/petstore.yaml"),

        ("comprehensive-schemas", "valid/comprehensive-schemas.yaml"),
        ("delete-with-response-schema", "valid/delete-with-response-schema.yaml"),
        ("duplicate-param-names", "valid/duplicate-param-names.yaml"),
        ("interface-with-enum-reference", "valid/interface-with-enum-reference.yaml"),
        ("minimal", "valid/minimal.yaml"),
        ("naming-conventions", "valid/naming-conventions.yaml"),
        ("server-object", "valid/server-object.yaml"),

        ("recursive-json-all-optional-properties", "valid/recursive-json/all-optional-properties.yaml"),
        ("recursive-json-all-optional-properties", "valid/recursive-json/all-optional-properties.yaml"),
        ("recursive-json-all-optional-properties", "valid/recursive-json/all-optional-properties.yaml"),
        ("recursive-json-array-of-inline-objects", "valid/recursive-json/array-of-inline-objects.yaml"),
        ("recursive-json-array-of-referenced-types", "valid/recursive-json/array-of-referenced-types.yaml"),
        ("recursive-json-array-with-reference-property", "valid/recursive-json/array-with-reference-property.yaml"),
        ("recursive-json-complex-array-structure", "valid/recursive-json/complex-array-structure.yaml"),
        ("recursive-json-deeply-nested-inline", "valid/recursive-json/deeply-nested-inline.yaml"),
        ("recursive-json-empty-array", "valid/recursive-json/empty-array.yaml"),
        ("recursive-json-inline-object", "valid/recursive-json/inline-object.yaml"),
        ("recursive-json-inline-object-with-array", "valid/recursive-json/inline-object-with-array.yaml"),
        ("recursive-json-mixed-property-types", "valid/recursive-json/mixed-property-types.yaml"),
        ("recursive-json-mixed-property-types", "valid/recursive-json/mixed-property-types.yaml"),
        ("recursive-json-nested-object-reference", "valid/recursive-json/nested-object-reference.yaml"),
        ("recursive-json-optional-array-of-inline-objects", "valid/recursive-json/optional-array-of-inline-objects.yaml"),
        ("recursive-json-optional-array-of-referenced-types", "valid/recursive-json/optional-array-of-referenced-types.yaml"),
        ("recursive-json-optional-inline-object", "valid/recursive-json/optional-inline-object.yaml"),
        ("recursive-json-optional-nested-object-reference", "valid/recursive-json/optional-nested-object-reference.yaml"),
        ("recursive-json-primitive-array", "valid/recursive-json/primitive-array.yaml"),
        ("recursive-json-primitive-array", "valid/recursive-json/primitive-array.yaml"),

        ("type-aliases-complex-union", "valid/type-aliases/complex-union.yaml"),
        ("type-aliases-intersection-allof", "valid/type-aliases/intersection-allof.yaml"),
        ("type-aliases-nested-union", "valid/type-aliases/nested-union.yaml"),
        ("type-aliases-simple-type-alias", "valid/type-aliases/simple-type-alias.yaml"),
        ("type-aliases-union-mixed", "valid/type-aliases/union-mixed.yaml"),
        ("type-aliases-union-with-interfaces", "valid/type-aliases/union-with-interfaces.yaml"),
        ("type-aliases-union-with-primitives", "valid/type-aliases/union-with-primitives.yaml"),
    ]
    .into_iter()
    .collect()
}

/// Generate and run golden file tests from the test cases HashMap
fn run_golden_test(spec_name: &str, fixture_path: &str) {
    if let Err(e) = test_golden_files(spec_name, fixture_path) {
        println!("Golden test failed for {}: {}", spec_name, e);
        process::exit(1);
    }
}

// Generate test functions from the HashMap
macro_rules! generate_golden_tests {
    ($($test_name:ident: $spec_name:expr),* $(,)?) => {
        $(
            #[test]
            #[traced_test]
            fn $test_name() {
                let test_cases = get_golden_test_cases();
                run_golden_test($spec_name, test_cases.get($spec_name).unwrap());
            }
        )*
    };
}

generate_golden_tests! {
    test_petstore_golden: "petstore",

    test_comprehensive_schemas_golden: "comprehensive-schemas",
    test_delete_with_response_schema_golden: "delete-with-response-schema",
    test_duplicate_param_names_golden: "duplicate-param-names",
    test_interface_with_enum_reference_golden: "interface-with-enum-reference",
    test_minimal_golden: "minimal",
    test_naming_conventions_golden: "naming-conventions",
    test_server_object_golden: "server-object",

    test_recursive_json_all_optional_properties_golden: "recursive-json-all-optional-properties",
    test_recursive_json_array_of_inline_objects_golden: "recursive-json-array-of-inline-objects",
    test_recursive_json_array_of_referenced_types_golden: "recursive-json-array-of-referenced-types",
    test_recursive_json_array_with_reference_property_golden: "recursive-json-array-with-reference-property",
    test_recursive_json_complex_array_structure_golden: "recursive-json-complex-array-structure",
    test_recursive_json_deeply_nested_inline_golden: "recursive-json-deeply-nested-inline",
    test_recursive_json_empty_array_golden: "recursive-json-empty-array",
    test_recursive_json_inline_object_golden: "recursive-json-inline-object",
    test_recursive_json_inline_object_with_array_golden: "recursive-json-inline-object-with-array",
    test_recursive_json_mixed_property_types_golden: "recursive-json-mixed-property-types",
    test_recursive_json_nested_object_reference_golden: "recursive-json-nested-object-reference",
    test_recursive_json_optional_array_of_inline_objects_golden: "recursive-json-optional-array-of-inline-objects",
    test_recursive_json_optional_array_of_referenced_types_golden: "recursive-json-optional-array-of-referenced-types",
    test_recursive_json_optional_inline_object_golden: "recursive-json-optional-inline-object",
    test_recursive_json_optional_nested_object_reference_golden: "recursive-json-optional-nested-object-reference",
    test_recursive_json_primitive_array_golden: "recursive-json-primitive-array",

    test_type_aliases_complex_union_golden: "type-aliases-complex-union",
    test_type_aliases_intersection_allof_golden: "type-aliases-intersection-allof",
    test_type_aliases_nested_union_golden: "type-aliases-nested-union",
    test_type_aliases_simple_type_alias_golden: "type-aliases-simple-type-alias",
    test_type_aliases_union_mixed_golden: "type-aliases-union-mixed",
    test_type_aliases_union_with_interfaces_golden: "type-aliases-union-with-interfaces",
    test_type_aliases_union_with_primitives_golden: "type-aliases-union-with-primitives",
}
