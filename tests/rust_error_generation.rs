//! Contracts for shared Rust error generation through the public generators.

use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
use openapi_nexus::generators::rust::aioduct::RustAioductCodeGenerator;
use openapi_nexus::generators::rust::reqwest::RustReqwestCodeGenerator;
use openapi_nexus::generators::rust::ureq::RustUreqCodeGenerator;
use openapi_nexus::generators::{ArtifactKind, GeneratorError};
use openapi_nexus::ir::types::{IrPrimitive, IrResponse, IrSpec, IrTypeExpr};
use openapi_nexus::test_utils::read_fixture;
use sigil_stitch::error::SigilStitchError;

fn generators() -> [Box<dyn CodeGenerator>; 3] {
    [
        Box::new(RustReqwestCodeGenerator::new(toml::value::Table::new())),
        Box::new(RustUreqCodeGenerator::new(toml::value::Table::new())),
        Box::new(RustAioductCodeGenerator::new(toml::value::Table::new())),
    ]
}

fn minimal_ir() -> IrSpec {
    let parsed = openapi_nexus::parser::parse_content_yaml(&read_fixture("valid/minimal.yaml"))
        .expect("minimal fixture parses");
    openapi_nexus::ir::lower::lower(parsed).expect("minimal fixture lowers")
}

fn payload_ir() -> IrSpec {
    let fixture = read_fixture("valid/rust-error-payload-shapes.yaml");
    let parsed =
        openapi_nexus::parser::parse_content_yaml(&fixture).expect("payload fixture parses");
    openapi_nexus::ir::lower::lower(parsed).expect("payload fixture lowers")
}

// Value: protects=Rust operation errors retain payload types and exact enum spelling;
// fails_when=arrays/maps/nullability/aliases/media overrides lose their payload shape;
// why_new=existing error goldens use named models, not composite inline payloads;
// seam=CodeGenerator::generate (all Rust backends); no_claim=runtime HTTP decoding.
#[test]
fn composite_error_payloads_preserve_their_generated_types() {
    let ir = payload_ir();
    let expected_enum = r#"pub enum GetPayloadsError {
    Status410(ApiError<Vec<crate::models::ErrorDetail>>),
    Status411(ApiError<Option<Vec<crate::models::ErrorDetail>>>),
    Status412(ApiError<std::collections::HashMap<String, Vec<crate::models::ErrorDetail>>>),
    Status413(ApiError<String>),
    Status414(ApiError<i32>),
    Status415(ApiError<crate::models::ErrorAlias>),
    Status416(ApiError<std::collections::HashMap<String, crate::models::VeryLongErrorPayloadModelNameThatFillsTheRenderWidth>>),
    Status417(ApiError<()>),
    Status418(ApiError<String>),
    Status419(ApiError<Vec<u8>>),
    Status420(ApiError<serde_json::Value>),
    Status421(ApiError<String>),
    Validation(ApiError<Vec<u8>>),
    Unexpected(ApiError<Vec<u8>>),
    Transport(Error),
}"#;

    for generator in generators() {
        let backend = generator.generator_type();
        let files = generator.generate(&ir).expect("payload fixture generates");
        let api = files
            .iter()
            .find(|file| file.filename == "payloads.rs")
            .expect("payloads API is generated");
        let start = api
            .content
            .find("pub enum GetPayloadsError {")
            .expect("error enum exists");
        let end = api.content[start..].find("\n}").expect("enum closes") + start + 2;
        assert_eq!(&api.content[start..end], expected_enum, "{backend}");
    }
}

// Value: protects=recursive error payload wrappers survive the IR-to-source boundary;
// fails_when=nullable named references, nested maps, or alias handling changes;
// why_new=OpenAPI lowering can normalize these combinations before generation;
// seam=CodeGenerator::generate (all Rust backends); no_claim=OpenAPI lowering correctness.
#[test]
fn nested_ir_error_payloads_preserve_nullability_and_aliases() {
    let cases = [
        (
            IrTypeExpr::Nullable(Box::new(IrTypeExpr::Named("ErrorDetail".to_string()))),
            "Option<crate::models::ErrorDetail>",
        ),
        (
            IrTypeExpr::Map(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Array(
                Box::new(IrTypeExpr::Named("ErrorDetail".to_string())),
            ))))),
            "std::collections::HashMap<String, Option<Vec<crate::models::ErrorDetail>>>",
        ),
        (
            IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Map(
                Box::new(IrTypeExpr::Named("String".to_string())),
            ))))),
            "Vec<Option<std::collections::HashMap<String, String>>>",
        ),
        (
            IrTypeExpr::Array(Box::new(IrTypeExpr::Named("ErrorAlias".to_string()))),
            "Vec<crate::models::ErrorAlias>",
        ),
        (IrTypeExpr::StringLiteral("expired".to_string()), "String"),
        (
            IrTypeExpr::Union(vec![
                IrTypeExpr::Primitive(IrPrimitive::Integer),
                IrTypeExpr::Primitive(IrPrimitive::Boolean),
            ]),
            "serde_json::Value",
        ),
    ];

    for (payload, expected_type) in cases {
        let mut ir = payload_ir();
        let response = ir.operations[0]
            .responses
            .iter_mut()
            .find(|response| response.status == "410")
            .expect("array error response exists");
        response
            .content
            .insert("application/json".to_string(), payload);

        for generator in generators() {
            let backend = generator.generator_type();
            let files = generator
                .generate(&ir)
                .expect("nested IR payload generates");
            let api = files
                .iter()
                .find(|file| file.filename == "payloads.rs")
                .expect("payloads API exists");
            let variant = api
                .content
                .lines()
                .find(|line| line.trim_start().starts_with("Status410("))
                .expect("410 variant exists");
            assert_eq!(
                variant,
                format!("    Status410(ApiError<{expected_type}>),"),
                "{backend}"
            );
        }
    }
}

// Value: protects=public Rust generators return rendering errors without panicking;
// fails_when=file rendering uses expect or loses API/source context;
// why_new=successful goldens never exercise rejected sigil-stitch types;
// seam=CodeGenerator::generate (all Rust backends); no_claim=valid OpenAPI input rejection.
#[test]
fn invalid_error_payload_returns_a_contextual_emission_error() {
    let mut ir = minimal_ir();
    ir.operations[0].responses.push(IrResponse {
        status: "400".to_string(),
        description: "Invalid lowered type reference".to_string(),
        content: indexmap::IndexMap::from([(
            "application/json".to_string(),
            IrTypeExpr::Named(String::new()),
        )]),
        item_content: indexmap::IndexMap::new(),
        headers: indexmap::IndexMap::new(),
    });

    for generator in generators() {
        let backend = generator.generator_type();
        let error = generator
            .generate(&ir)
            .expect_err("an empty qualified type name must be rejected, not rendered");
        match error {
            GeneratorError::Emission {
                artifact,
                name,
                source,
            } => {
                assert!(
                    matches!(artifact, ArtifactKind::Api),
                    "{backend}: {artifact}"
                );
                assert_eq!(name, "default API", "{backend}");
                match *source {
                    SigilStitchError::InvalidTypeName { context, reason } => {
                        assert!(!context.is_empty(), "{backend}: missing render context");
                        assert_eq!(reason, "imported name must not be blank", "{backend}");
                    }
                    other => panic!("{backend}: original type diagnostic was lost: {other:?}"),
                }
            }
            other => panic!("{backend}: expected a typed emission error, got {other:?}"),
        }
    }
}
