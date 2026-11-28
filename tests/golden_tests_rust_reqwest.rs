//! Golden file tests for Rust reqwest generator
//!
//! These tests compare generated Rust code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_reqwest

use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;

mod support;
type GoldenGenerator = RustReqwestCodeGenerator;
include!(concat!(env!("OUT_DIR"), "/golden_cases_rust_reqwest.rs"));

#[test]
fn ordinary_union_predicates_keep_authored_string_constants_in_both_openapi_versions() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use openapi_nexus::ir::lower::lower;
    use openapi_nexus::parser::parse_content_yaml;

    let fixture = include_str!("fixtures/valid/rust-union-string-const.yaml");
    let model = |input: &str, strategy: &str, filename: &str| {
        let ir = lower(parse_content_yaml(input).unwrap()).unwrap();
        RustReqwestCodeGenerator::new(toml::toml! { untagged_union_strategy = strategy })
            .generate(&ir)
            .unwrap()
            .into_iter()
            .find(|file| file.filename == filename)
            .unwrap()
            .content
    };

    for version in ["3.1.0", "3.2.0"] {
        let input = fixture.replacen("3.1.0", version, 1);
        for strategy in ["firstMatch", "strict"] {
            let output = model(&input, strategy, "format_choice.rs");
            assert!(output.contains("#[serde(untagged)]"));
            assert!(output.contains("FormatChoiceMember1(super::FormatChoiceMember1)"));
            assert!(output.contains("FormatChoiceMember2(super::FormatChoiceMember2)"));
            assert!(output.contains("FormatChoiceMember3(super::FormatChoiceMember3)"));
            assert!(output.contains("candidate.as_str() == Some(\"plain\")"));
            assert!(output.contains("candidate.as_str() == Some(\"structured\")"));
            assert_eq!(
                output.contains("multiple concrete members match"),
                strategy == "strict"
            );
            let single = model(&input, strategy, "single_choice.rs");
            assert!(single.contains("candidate.as_str() == Some(\"plain\")"));
        }
        let raw = model(&input, "raw", "format_choice.rs");
        assert!(raw.contains("Raw(serde_json::Value)"));
        assert!(!raw.contains("__nexus_matches"));
    }

    // A sibling with the same property but no finite constraint still accepts
    // general strings. It does not erase the other branch's authored literal.
    let unconstrained = fixture.replace(", const: structured", "");
    let output = model(&unconstrained, "firstMatch", "format_choice.rs");
    assert!(output.contains("candidate.is_string()"));
    assert!(output.contains("candidate.as_str() == Some(\"plain\")"));
}

#[test]
fn inferred_tagged_wire_projects_only_the_fields_that_need_it() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use std::collections::HashSet;

    let document = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/rust-tagged-wire-boundaries.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(document).unwrap();
    for strategy in ["firstMatch", "strict", "raw"] {
        let files = RustReqwestCodeGenerator::new(toml::toml! { tagged_union_strategy = strategy })
            .generate(&ir)
            .unwrap();
        let model = |filename| {
            files
                .iter()
                .find(|file| file.filename == filename)
                .unwrap()
                .content
                .as_str()
        };
        if strategy == "raw" {
            let envelope = model("envelope.rs");
            assert!(envelope.contains("pub alias: Option<super::ChoiceAlias>"));
            assert!(envelope.contains("pub choice: Option<super::Choice>"));
            assert!(envelope.contains("deserialize_with = \"__nexus_deserialize_present_raw\""));
            assert!(!model("choice.rs").contains("__nexus_wire_match"));
        } else {
            let choice = model("choice.rs");
            assert!(choice.contains("payload: super::ChoiceStructured"));
            assert!(choice.contains("Legacy(super::Legacy)"));
            assert!(choice.contains("Sequence(super::Sequence)"));
            assert!(model("composed_choice.rs").contains("payload: super::Combined"));
            assert!(model("combined.rs").contains("pub index_part: super::IndexPart"));
            assert!(model("index_part.rs").contains("pub entries: Option<super::DetailMapAlias>"));
            assert!(model("projected_choice.rs").contains("UntaggedValues(serde_json::Value)"));
            let names: Vec<_> = model("sample.rs")
                .lines()
                .filter_map(|line| line.strip_prefix("pub enum SampleDiscriminator"))
                .collect();
            assert_eq!(names.len(), 12);
            assert_eq!(names.into_iter().collect::<HashSet<_>>().len(), 12);
        }
    }
}
