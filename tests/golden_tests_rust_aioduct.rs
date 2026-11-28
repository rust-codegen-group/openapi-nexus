//! Golden file tests for Rust aioduct generator
//!
//! These tests compare generated Rust code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_aioduct

use openapi_nexus::generators::rust::aioduct::RustAioductCodeGenerator;

mod support;
type GoldenGenerator = RustAioductCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_rust_aioduct.rs"));

#[test]
fn test_inferred_tagged_policies_generate_distinct_model_contracts() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/rust-inferred-tagged-wire.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let configs: toml::value::Table =
        toml::from_str(include_str!("generated-clients/rust/union-policies.toml")).unwrap();
    for strategy in ["firstMatch", "strict", "raw"] {
        let mut config = configs["rust-aioduct"].as_table().unwrap().clone();
        config.insert("tagged_union_strategy".into(), strategy.into());
        let files = RustAioductCodeGenerator::new(config).generate(&ir).unwrap();
        let model = &files
            .iter()
            .find(|file| file.filename == "ambiguous_tagged.rs")
            .unwrap()
            .content;
        if strategy == "raw" {
            assert!(model.contains("Raw(serde_json::Value)"));
            assert!(!model.contains("impl<'de> serde::Deserialize<'de>"));
        } else {
            assert!(model.contains("impl<'de> serde::Deserialize<'de>"));
            assert_eq!(
                model.contains("multiple concrete members match"),
                strategy == "strict"
            );
        }
    }
}
