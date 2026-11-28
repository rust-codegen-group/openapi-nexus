//! Golden file tests for Kotlin OkHttp generator
//!
//! These tests compare generated Kotlin code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_kotlin_okhttp

use openapi_nexus::generators::kotlin::okhttp::KotlinOkhttpCodeGenerator;
use openapi_nexus::test_utils::{generate_files, read_fixture};

mod support;
type GoldenGenerator = KotlinOkhttpCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_kotlin_okhttp.rs"));

#[test]
fn kotlin_primary_constructor_annotations_preserve_wire_names() {
    let generator = KotlinOkhttpCodeGenerator::new(toml::value::Table::new());
    let files = generate_files(&generator, &read_fixture("valid/naming-conventions.yaml"))
        .expect("naming-conventions fixture generates");
    let model = files
        .get("models/NamingConventionTest.kt")
        .expect("NamingConventionTest model is generated");

    assert!(
        model.contains("@field:SerializedName(\"snake_case_field\")"),
        "wire-name annotation must target the generated backing field:\n{model}"
    );
    assert!(
        model.contains("val snakeCaseField: String"),
        "the generated property keeps its Kotlin identifier:\n{model}"
    );
}

#[test]
fn policy_changes_are_confined_to_untagged_union_adapters_and_raw_cases() {
    use heck::ToPascalCase;
    use openapi_nexus::ir::{lower::lower, types::IrSchemaKind};
    use openapi_nexus::parser::parse_content_yaml;

    for fixture in [
        "valid/comprehensive-schemas.yaml",
        "valid/java-union-policies.yaml",
        "valid/type-aliases/discriminated-union-mixed-unit-and-allof.yaml",
    ] {
        let source = read_fixture(fixture);
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let unions: std::collections::HashSet<_> = ir
            .schemas
            .values()
            .filter(|schema| matches!(schema.kind, IrSchemaKind::Union(_)))
            .map(|schema| format!("models/{}.kt", schema.name.to_pascal_case()))
            .collect();
        let typed =
            generate_files(&KotlinOkhttpCodeGenerator::new(Default::default()), &source).unwrap();
        for policy in ["strict", "raw"] {
            let generator = KotlinOkhttpCodeGenerator::new(
                toml::from_str(&format!("untagged_union_strategy = {policy:?}")).unwrap(),
            );
            let changed = generate_files(&generator, &source).unwrap();
            assert_eq!(typed.len(), changed.len());
            for (file, content) in &typed {
                if !unions.contains(file) {
                    assert_eq!(content, &changed[file], "{fixture} {policy} {file}");
                } else if policy == "strict" {
                    assert_eq!(
                        content.split("\nprivate class").next(),
                        changed[file].split("\nprivate class").next(),
                        "typed API {file}"
                    );
                }
            }
        }
    }
}
