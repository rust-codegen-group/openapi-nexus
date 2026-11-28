//! Golden file tests for TypeScript Fetch generator
//!
//! These tests compare generated TypeScript code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch

use openapi_nexus::generators::request_inputs::RequestInputPlan;
use openapi_nexus::generators::typescript::fetch::TypeScriptFetchCodeGenerator;

mod support;
type GoldenGenerator = TypeScriptFetchCodeGenerator;
include!(concat!(
    env!("OUT_DIR"),
    "/golden_cases_typescript_fetch.rs"
));
include!(concat!(env!("OUT_DIR"), "/golden_fragments.rs"));

#[test]
fn test_camel_case_untagged_union_delegates_to_member_converters() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let config = toml::from_str("property_naming = 'camelCase'").unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/typescript-untagged-union.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let files = generator.generate(&ir).unwrap();
    let union = files
        .iter()
        .find(|file| file.filename == "Choice.ts")
        .unwrap();
    assert!(union.content.contains("payloadAFromJSON("));
    assert!(union.content.contains("payloadBToJSON("));
}

#[test]
fn test_invalid_untagged_union_member_retains_model_emission_context() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use openapi_nexus::generators::{ArtifactKind, GeneratorError};
    use openapi_nexus::ir::types::{IrSchemaKind, IrTypeExpr};
    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/typescript-untagged-union.yaml"
    ))
    .unwrap();
    let mut ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let IrSchemaKind::Union(union) = &mut ir.schemas.get_mut("Choice").unwrap().kind else {
        panic!("untagged union")
    };
    union.members.push(IrTypeExpr::Named(String::new()));
    let generator =
        TypeScriptFetchCodeGenerator::new(toml::from_str("property_naming = 'camelCase'").unwrap());
    let error = generator
        .generate(&ir)
        .expect_err("invalid type must retain its error, not panic");
    assert!(
        matches!(error, GeneratorError::Emission { artifact: ArtifactKind::Model, name, .. } if name == "Choice")
    );
}

// ===========================================================================
// IR pipeline integration tests
// ===========================================================================

#[test]
fn test_ir_model_generation_produces_files() {
    let yaml = r#"
openapi: "3.1.0"
info:
  title: IR Test API
  version: "1.0.0"
components:
  schemas:
    User:
      type: object
      properties:
        id:
          type: integer
        name:
          type: string
        email:
          type: string
      required:
        - id
        - name
    Status:
      type: string
      enum:
        - active
        - inactive
        - suspended
"#;

    let parsed = openapi_nexus::parser::parse_content_yaml(yaml).unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();

    let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
    let request_inputs = RequestInputPlan::empty();
    let files = generator
        .generate_models_from_ir(&ir, &request_inputs)
        .unwrap();

    assert!(
        files.len() >= 3,
        "Expected at least 3 files (User, Status, index), got {}",
        files.len()
    );

    let filenames: Vec<&str> = files.iter().map(|f| f.filename.as_str()).collect();
    assert!(
        filenames.iter().any(|f| f.contains("User")),
        "Expected a User model file, got: {:?}",
        filenames
    );
    assert!(
        filenames.iter().any(|f| f.contains("Status")),
        "Expected a Status model file, got: {:?}",
        filenames
    );

    let user_file = files.iter().find(|f| f.filename.contains("User")).unwrap();
    assert!(
        user_file.content.contains("export interface User"),
        "User file should contain 'export interface User', got:\n{}",
        user_file.content
    );
    assert!(
        user_file.content.contains("id:") || user_file.content.contains("id?:"),
        "User file should contain 'id' property"
    );

    let status_file = files
        .iter()
        .find(|f| f.filename.contains("Status"))
        .unwrap();
    assert!(
        status_file.content.contains("Status"),
        "Status file should contain 'Status'"
    );
}

#[test]
fn test_ir_model_nullable_types() {
    let yaml = r#"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
components:
  schemas:
    Profile:
      type: object
      properties:
        bio:
          type:
            - string
            - "null"
      required:
        - bio
"#;

    let parsed = openapi_nexus::parser::parse_content_yaml(yaml).unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();

    let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
    let request_inputs = RequestInputPlan::empty();
    let files = generator
        .generate_models_from_ir(&ir, &request_inputs)
        .unwrap();

    let profile_file = files
        .iter()
        .find(|f| f.filename.contains("Profile"))
        .unwrap();
    assert!(
        profile_file.content.contains("null"),
        "Profile file should contain null type for nullable property, got:\n{}",
        profile_file.content
    );
}

#[test]
fn test_ir_model_ref_alias() {
    let yaml = r##"
openapi: "3.1.0"
info:
  title: Test
  version: "1.0.0"
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
    MyPet:
      $ref: "#/components/schemas/Pet"
"##;

    let parsed = openapi_nexus::parser::parse_content_yaml(yaml).unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();

    let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
    let request_inputs = RequestInputPlan::empty();
    let files = generator
        .generate_models_from_ir(&ir, &request_inputs)
        .unwrap();

    assert!(
        files.len() >= 3,
        "Expected at least 3 files, got {}",
        files.len()
    );

    let my_pet_file = files.iter().find(|f| f.filename.contains("MyPet")).unwrap();
    assert!(
        my_pet_file.content.contains("Pet"),
        "MyPet file should reference Pet, got:\n{}",
        my_pet_file.content
    );
}

#[test]
fn test_self_referential_object_keeps_self_references_local() {
    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/recursive-json/self-referential-object.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();

    for (config, camel_case) in [
        (toml::value::Table::new(), false),
        (
            toml::from_str("property_naming = \"camelCase\"").unwrap(),
            true,
        ),
    ] {
        let generator = TypeScriptFetchCodeGenerator::new(config);
        let files = generator
            .generate_models_from_ir(&ir, &RequestInputPlan::empty())
            .unwrap();
        let foo_file = files
            .iter()
            .find(|file| file.filename.ends_with("Foo.ts"))
            .unwrap();

        assert!(
            !foo_file.content.contains("from './Foo'"),
            "Foo.ts must not import its own declarations:\n{}",
            foo_file.content
        );
        assert!(
            foo_file.content.contains("from './Bar'"),
            "Foo.ts must retain imports for other models:\n{}",
            foo_file.content
        );
        assert!(
            foo_file.content.contains("readonly Foo[]"),
            "Foo.children must retain its recursive type:\n{}",
            foo_file.content
        );
        if camel_case {
            assert!(
                !files
                    .iter()
                    .any(|file| file.filename.ends_with("unionMatch.ts")),
                "a union-free recursive model must not emit unused matchers"
            );
            assert!(
                foo_file
                    .content
                    .contains("readonly children?: readonly Foo$Wire[];"),
                "Foo$Wire.children must use the local wire type:\n{}",
                foo_file.content
            );
            assert!(
                foo_file
                    .content
                    .contains("convert$0FromJSON(json.children)"),
                "Foo's decoder must delegate its array through the planned helper:\n{}",
                foo_file.content
            );
            assert!(
                foo_file.content.contains("convert$0ToJSON(value.children)"),
                "Foo's encoder must delegate its array through the planned helper:\n{}",
                foo_file.content
            );
            let helpers = files
                .iter()
                .find(|file| file.filename.ends_with("valueConversion$0.ts"))
                .unwrap();
            assert!(
                helpers
                    .content
                    .contains("import type { Foo, Foo$Wire } from '../models/Foo';")
            );
            assert!(
                helpers
                    .content
                    .contains("import { fooFromJSON, fooToJSON } from '../models/Foo';")
            );
            assert!(
                helpers.content.contains("return json.map(fooFromJSON);"),
                "the helper must retain the original recursive decoder: {}",
                helpers.content
            );
            assert!(
                helpers.content.contains("return value.map(fooToJSON);"),
                "the helper must retain the original recursive encoder: {}",
                helpers.content
            );
        }
    }
}

// Deliberately invalid lowered IR, not a valid OpenAPI input or a production hook.
#[test]
fn test_invalid_error_payload_retains_api_emission_context() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    use openapi_nexus::generators::{ArtifactKind, GeneratorError};
    use openapi_nexus::ir::types::IrTypeExpr;
    use sigil_stitch::error::SigilStitchError;

    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/typescript-response-dispatch.yaml"
    ))
    .expect("response dispatch fixture parses");
    let mut ir = openapi_nexus::ir::lower::lower(parsed).expect("response dispatch fixture lowers");
    let response = ir
        .operations
        .iter_mut()
        .find(|op| op.operation_id == "readMixed")
        .expect("mixed response operation exists")
        .responses
        .iter_mut()
        .find(|response| response.status == "400")
        .expect("named error response exists");
    response.content.insert(
        "application/json".to_string(),
        IrTypeExpr::Named(String::new()),
    );
    let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
    let error = generator
        .generate(&ir)
        .expect_err("invalid error payload must return an error, not panic");
    match error {
        GeneratorError::Emission {
            artifact,
            name,
            source,
        } => {
            assert!(matches!(artifact, ArtifactKind::Api));
            assert_eq!(name, "Responses API");
            match *source {
                SigilStitchError::InvalidTypeName { context, reason } => {
                    assert!(!context.is_empty(), "original render context is retained");
                    assert_eq!(reason, "imported name must not be blank");
                }
                other => panic!("original type diagnostic was lost: {other:?}"),
            }
        }
        other => panic!("expected a contextual API emission error, got {other:?}"),
    }
}

#[test]
fn test_tagged_strict_rejects_converter_free_preserve_mode() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;
    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/optional-discriminator-union.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let config: toml::value::Table = toml::from_str("tagged_union_strategy = 'strict'").unwrap();
    let error = TypeScriptFetchCodeGenerator::new(config)
        .generate(&ir)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires property_naming = 'camelCase'")
    );
}

#[test]
fn test_authored_tag_policies_generate_distinct_runtime_contracts() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let parsed = openapi_nexus::parser::parse_content_yaml(include_str!(
        "fixtures/valid/inferred-union-selection.yaml"
    ))
    .unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    for (naming, strategy) in [
        ("camelCase", "strict"),
        ("camelCase", "raw"),
        ("preserve", "firstMatch"),
    ] {
        let config = toml::from_str(&format!(
            "property_naming = '{naming}'\ntagged_union_strategy = '{strategy}'\nemit_type_guards = true"
        ))
        .unwrap();
        let files = TypeScriptFetchCodeGenerator::new(config)
            .generate(&ir)
            .unwrap();
        let choice = &files
            .iter()
            .find(|file| file.filename == "GatedChoice.ts")
            .unwrap()
            .content;
        match (naming, strategy) {
            ("camelCase", "strict") => {
                assert!(choice.contains("Ambiguous GatedChoice tagged union value"));
                assert!(choice.contains("gatedChoiceTypedFromJSON("));
            }
            ("camelCase", "raw") => {
                assert!(choice.contains("return json;"));
                assert!(!choice.contains("gatedChoiceTypedFromJSON("));
            }
            ("preserve", _) => assert!(!choice.contains("function gatedChoiceFromJSON(")),
            _ => unreachable!(),
        }
    }
}

#[test]
fn authored_projection_fixture_exports_distinct_guards_and_naming_contracts() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let source = include_str!("fixtures/valid/typescript-authored-tagged-projections.yaml");
    let ir =
        openapi_nexus::ir::lower::lower(openapi_nexus::parser::parse_content_yaml(source).unwrap())
            .unwrap();
    for (naming, strategy) in [
        ("camelCase", "firstMatch"),
        ("camelCase", "strict"),
        ("camelCase", "raw"),
        ("preserve", "firstMatch"),
    ] {
        let config = toml::from_str(&format!(
            "property_naming = '{naming}'\ntagged_union_strategy = '{strategy}'\nemit_type_guards = true"
        ))
        .unwrap();
        let files = TypeScriptFetchCodeGenerator::new(config)
            .generate(&ir)
            .unwrap();
        let model = |name: &str| {
            &files
                .iter()
                .find(|file| file.filename == name)
                .unwrap()
                .content
        };
        let barrel = model("models/index.ts");
        let exports = barrel
            .split(';')
            .find(|statement| {
                statement.contains("from './GuardChoice'") && statement.contains("isTwin")
            })
            .expect("guards re-exported from the model barrel");
        let guards = model("GuardChoice.ts");
        for guard in ["isTwin", "isTwin2", "isTwin3"] {
            assert!(
                exports
                    .split(|ch: char| !ch.is_ascii_alphanumeric())
                    .any(|token| token == guard)
            );
            assert_eq!(
                guards.matches(&format!("export function {guard}(")).count(),
                1
            );
        }
        let fork = model("Fork.ts");
        assert!(fork.contains("Record<'scope_left', unknown>"));
        assert_eq!(
            fork.contains("Record<'scopeLeft', unknown>"),
            naming == "camelCase" && strategy != "raw",
        );
        assert_eq!(
            fork.contains("export function forkFromJSON("),
            naming == "camelCase"
        );
    }
}

#[test]
fn authored_whole_alternatives_keep_their_native_type_and_converters() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let source = include_str!("fixtures/valid/typescript-authored-whole-and-finite.yaml");
    let ir =
        openapi_nexus::ir::lower::lower(openapi_nexus::parser::parse_content_yaml(source).unwrap())
            .unwrap();
    let config = toml::from_str(
        "property_naming = 'camelCase'\ntagged_union_strategy = 'strict'\nemit_type_guards = true\ntoolchain = 'vp'",
    ).unwrap();
    let files = TypeScriptFetchCodeGenerator::new(config)
        .generate(&ir)
        .unwrap();
    let model = files
        .iter()
        .find(|file| file.filename == "Choice.ts")
        .unwrap();
    assert_eq!(
        model.content,
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/",
            golden_fragment_path!(typescript_authored_whole_choice)
        ))
    );
}

#[test]
fn authored_constants_follow_the_selected_named_payload_policy() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let source = include_str!("fixtures/valid/typescript-named-literal-policies.yaml");
    let ir =
        openapi_nexus::ir::lower::lower(openapi_nexus::parser::parse_content_yaml(source).unwrap())
            .unwrap();
    for policy in ["firstMatch", "strict", "raw"] {
        let config = toml::from_str(&format!(
            "property_naming = 'camelCase'\ntagged_union_strategy = 'strict'\nuntagged_union_strategy = '{policy}'\nemit_type_guards = true"
        ))
        .unwrap();
        let files = TypeScriptFetchCodeGenerator::new(config)
            .generate(&ir)
            .unwrap();
        let output = &files
            .iter()
            .find(|file| file.filename.ends_with("unionMatch.ts"))
            .expect("authored constraints have a generated matcher")
            .content;
        let checks_key = |direction: &str, key: &str| {
            output.split("export function ").any(|declaration| {
                declaration
                    .lines()
                    .next()
                    .is_some_and(|signature| signature.contains(direction))
                    && declaration.contains(&format!("hasOwnProperty.call(literal, '{key}')"))
            })
        };
        // Wire constants keep their authored spelling in every policy.
        assert!(checks_key("FromJSON", "nullable_value"), "{policy}");
        // The selected structured payload converts its declared fields, but map
        // entries, extensions and opaque fallback keys are still data.
        assert_eq!(
            checks_key("ToJSON", "nullableValue"),
            policy != "raw",
            "{policy}"
        );
        for key in [
            "literal_key",
            "extension_key",
            "opaque_key",
            "dictionary_key",
        ] {
            assert!(checks_key("ToJSON", key), "{policy}: {key}");
        }
        // The common literal for AmbiguousPayload matches both structured
        // alternatives. Strict must not project it through a guessed member.
        assert_eq!(
            checks_key("ToJSON", "namedOnly"),
            policy == "firstMatch",
            "{policy}"
        );
    }
}

#[test]
fn composed_wire_codec_checks_competing_tags_after_conversion() {
    use openapi_nexus::codegen::traits::code_generator::CodeGenerator;

    let source = include_str!("fixtures/valid/rust-inferred-tagged-wire.yaml");
    let parsed = openapi_nexus::parser::parse_content_yaml(source).unwrap();
    let ir = openapi_nexus::ir::lower::lower(parsed).unwrap();
    let config = toml::from_str(
        "property_naming = 'camelCase'\ntagged_union_strategy = 'strict'\ntoolchain = 'vp'",
    )
    .unwrap();
    let files = TypeScriptFetchCodeGenerator::new(config)
        .generate(&ir)
        .unwrap();
    let model = &files
        .iter()
        .find(|file| file.filename == "ComposedTagged.ts")
        .unwrap()
        .content;
    for (direction, argument, ty) in [
        ("FromJSON", "json", "ComposedTaggedAlpha$Wire"),
        ("ToJSON", "value", "ComposedTaggedAlpha"),
    ] {
        assert!(model.contains(&format!(
            "return composedTaggedAlpha{direction}(\n      {argument} as unknown as {ty},\n    ) as ComposedTagged"
        )));
        assert!(model.contains(&format!(
            "const composed = composedTaggedAlphaAlt{direction}("
        )));
    }
    assert_eq!(
        model
            .matches("Conflicting inferred discriminator in payload")
            .count(),
        2
    );
}
