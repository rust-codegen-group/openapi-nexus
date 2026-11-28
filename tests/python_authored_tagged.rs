//! In-process contract for the public Python generators; native behavior lives in the consumers.

use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::codegen::traits::file_writer::{FileCategory, FileInfo};
use openapi_nexus::generators::python::{
    httpx::PythonHttpxCodeGenerator, requests::PythonRequestsCodeGenerator,
};
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;

include!(concat!(env!("OUT_DIR"), "/golden_fragments.rs"));

fn generators(strategy: &str) -> [Box<dyn CodeGenerator>; 2] {
    let config = toml::toml! { tagged_union_strategy = strategy };
    [
        Box::new(PythonHttpxCodeGenerator::new(config.clone())),
        Box::new(PythonRequestsCodeGenerator::new(config)),
    ]
}

fn model<'a>(files: &'a [FileInfo], name: &str) -> &'a str {
    &files
        .iter()
        .find(|file| file.category == FileCategory::Models && file.filename == name)
        .unwrap_or_else(|| panic!("missing generated Python model file {name}"))
        .content
}

#[test]
fn authored_models_have_the_same_public_case_and_helpers_for_both_transports() {
    let ir =
        lower(parse_content_yaml(include_str!("fixtures/valid/python-tagged-echo.yaml")).unwrap())
            .unwrap();
    for generator in generators("firstMatch") {
        let files = generator.generate(&ir).unwrap();
        assert_eq!(
            model(&files, "message.py"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/",
                golden_fragment_path!(python_authored_tagged_message)
            ))
        );
        assert!(model(&files, "__init__.py").contains("MessageTextCase as MessageTextCase"));
    }
}

#[test]
fn required_fixed_labels_with_common_constraints_keep_the_legacy_representation() {
    let source = r#"
openapi: 3.1.0
info: {title: Fixed compatibility, version: '1'}
paths: {}
components:
  schemas:
    FixedChoice:
      type: object
      required: [scope]
      oneOf:
        - allOf:
            - type: object
              required: [kind, label]
              properties:
                kind: {type: string, enum: [a]}
                label: {type: string}
        - allOf:
            - type: object
              required: [kind, count]
              properties:
                kind: {type: string, enum: [b]}
                count: {type: integer}
"#;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    for strategy in ["firstMatch", "strict", "raw"] {
        for generator in generators(strategy) {
            let files = generator.generate(&ir).unwrap();
            let choice = model(&files, "fixed_choice.py");
            assert!(choice.contains("type FixedChoice = FixedChoiceA | FixedChoiceB"));
            assert!(choice.contains("if _tag == \"a\":"));
            assert!(choice.contains("if _tag == \"b\":"));
            assert!(
                !files
                    .iter()
                    .any(|file| file.filename == "_tagged_support.py")
            );
        }
    }
}

#[test]
fn tagged_policies_generate_typed_extensions_and_fallback_or_an_unconverted_value() {
    let ir = lower(
        parse_content_yaml(include_str!("fixtures/valid/inferred-union-selection.yaml")).unwrap(),
    )
    .unwrap();
    for strategy in ["strict", "raw"] {
        for generator in generators(strategy) {
            let files = generator.generate(&ir).unwrap();
            let gated = model(&files, "gated_choice.py");
            let tree = model(&files, "tree.py");
            let has_runtime = files
                .iter()
                .any(|file| file.filename == "_tagged_support.py");
            if strategy == "strict" {
                assert!(has_runtime);
                assert!(gated.contains(
                    "type GatedChoice = GatedChoiceOpaquePayloadCase | GatedChoiceTypedCase"
                ));
                assert!(tree.contains("type Tree = TreeNodeCase | TreeLeafCase"));
                assert!(
                    model(&files, "tree_leaf.py")
                        .contains("additional_properties: _dict[_str, int]")
                );
                assert!(!model(&files, "_union.py").contains("branches, False, \"GatedChoice\""));
                assert!(model(&files, "_union.py").contains("branches, True, \"GatedChoice\""));
            } else {
                assert!(!has_runtime);
                assert!(gated.contains("type GatedChoice = object"));
                assert!(tree.contains("type Tree = object"));
                assert!(!model(&files, "__init__.py").contains("TreeLeafCase"));
            }
        }
    }
}
