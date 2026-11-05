//! Golden file tests for TypeScript Fetch generator
//!
//! These tests compare generated TypeScript code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch

use std::collections::HashMap;
use std::path::Path;

use tracing_test::traced_test;

use openapi_nexus::generators::request_inputs::RequestInputPlan;
use openapi_nexus::generators::typescript::fetch::TypeScriptFetchCodeGenerator;
use openapi_nexus::test_utils::{run_golden_test, test_cases_from_slice};

fn golden_dir() -> &'static Path {
    Path::new("tests/golden/typescript/typescript-fetch")
}

const UPDATE_HINT: &str = "UPDATE_GOLDEN=1 cargo test --test golden_tests_typescript_fetch";

#[rustfmt::skip]
fn get_golden_test_cases() -> HashMap<&'static str, &'static str> {
    test_cases_from_slice(&[
        ("petstore", "valid/petstore.yaml"),

        ("comprehensive-schemas", "valid/comprehensive-schemas.yaml"),
        ("delete-with-response-schema", "valid/delete-with-response-schema.yaml"),
        ("duplicate-param-names", "valid/duplicate-param-names.yaml"),
        ("interface-with-enum-reference", "valid/interface-with-enum-reference.yaml"),
        ("minimal", "valid/minimal.yaml"),
        ("multiple-similar-request-schemas", "valid/multiple-similar-request-schemas.yaml"),
        ("naming-conventions", "valid/naming-conventions.yaml"),
        ("request-body-content-types", "valid/request-body-content-types.yaml"),
        ("optional-request-bodies", "valid/optional-request-bodies.yaml"),
        ("binary-transfer-media-types", "valid/binary-transfer-media-types.yaml"),
        ("media-type-selection", "valid/media-type-selection.yaml"),
        ("multipart-explicit-encoding", "valid/multipart-explicit-encoding.yaml"),
        ("multipart-edge-cases", "valid/multipart-edge-cases.yaml"),
        ("multipart-nested-object-parts", "valid/multipart-nested-object-parts.yaml"),
        ("multipart-unsupported-schema", "valid/multipart-unsupported-schema.yaml"),
        ("server-object", "valid/server-object.yaml"),

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
        ("recursive-json-nested-object-reference", "valid/recursive-json/nested-object-reference.yaml"),
        ("recursive-json-optional-array-of-inline-objects", "valid/recursive-json/optional-array-of-inline-objects.yaml"),
        ("recursive-json-optional-array-of-referenced-types", "valid/recursive-json/optional-array-of-referenced-types.yaml"),
        ("recursive-json-optional-inline-object", "valid/recursive-json/optional-inline-object.yaml"),
        ("recursive-json-optional-nested-object-reference", "valid/recursive-json/optional-nested-object-reference.yaml"),
        ("recursive-json-primitive-array", "valid/recursive-json/primitive-array.yaml"),
        ("recursive-json-self-referential-object", "valid/recursive-json/self-referential-object.yaml"),

        ("type-aliases-complex-union", "valid/type-aliases/complex-union.yaml"),
        ("type-aliases-intersection-allof", "valid/type-aliases/intersection-allof.yaml"),
        ("type-aliases-intersection-with-nullable-reference", "valid/type-aliases/intersection-with-nullable-reference.yaml"),
        ("type-aliases-nested-union", "valid/type-aliases/nested-union.yaml"),
        ("type-aliases-simple-type-alias", "valid/type-aliases/simple-type-alias.yaml"),
        ("type-aliases-union-mixed", "valid/type-aliases/union-mixed.yaml"),
        ("type-aliases-union-with-any", "valid/type-aliases/union-with-any.yaml"),
        ("type-aliases-union-with-inline-objects", "valid/type-aliases/union-with-inline-objects.yaml"),
        ("type-aliases-union-with-interfaces", "valid/type-aliases/union-with-interfaces.yaml"),
        ("type-aliases-union-with-primitives", "valid/type-aliases/union-with-primitives.yaml"),
        ("type-aliases-discriminated-union-internally-tagged", "valid/type-aliases/discriminated-union-internally-tagged.yaml"),
        ("type-aliases-discriminated-union-with-refs", "valid/type-aliases/discriminated-union-with-refs.yaml"),
        ("type-aliases-discriminated-union-multiple", "valid/type-aliases/discriminated-union-multiple.yaml"),
        ("type-aliases-discriminated-union-long-names", "valid/type-aliases/discriminated-union-long-names.yaml"),
        ("type-aliases-discriminated-union-inline-discriminator-only", "valid/type-aliases/discriminated-union-inline-discriminator-only.yaml"),
        ("type-aliases-discriminated-union-mixed-unit-and-allof", "valid/type-aliases/discriminated-union-mixed-unit-and-allof.yaml"),

        ("response-body-default-and-exact", "valid/response-body/default-and-exact.yaml"),
        ("response-body-fallback", "valid/response-body/fallback.yaml"),
        ("response-body-multi-status-responses", "valid/response-body/multi-status-responses.yaml"),
        ("response-body-no-response-body", "valid/response-body/no-response-body.yaml"),
        ("typed-error-responses", "valid/typed-error-responses.yaml"),
        ("response-dispatch", "valid/typescript-response-dispatch.yaml"),
        ("api-interface", "valid/typescript-api-interface.yaml"),
        ("nested-nullable", "valid/nested-nullable.yaml"),

        ("additional-properties", "valid/additional-properties/additional-properties.yaml"),
        ("enum-repr", "valid/enum-repr/enum-repr.yaml"),
        ("query-param-enum", "valid/query/query-param-enum.yaml"),
        ("server-path-prefix", "valid/server-path-prefix.yaml"),
    ])
}

macro_rules! generate_golden_tests {
    ($($test_name:ident: $spec_name:expr),* $(,)?) => {
        $(
            #[test]
            #[traced_test]
            fn $test_name() {
                let cases = get_golden_test_cases();
                let generator = TypeScriptFetchCodeGenerator::new(toml::value::Table::new());
                run_golden_test(
                    &generator,
                    golden_dir(),
                    $spec_name,
                    cases.get($spec_name).unwrap(),
                    UPDATE_HINT,
                );
            }
        )*
    };
}

generate_golden_tests! {
    test_nested_nullable_golden: "nested-nullable",
    test_petstore_golden: "petstore",

    test_comprehensive_schemas_golden: "comprehensive-schemas",
    test_delete_with_response_schema_golden: "delete-with-response-schema",
    test_duplicate_param_names_golden: "duplicate-param-names",
    test_interface_with_enum_reference_golden: "interface-with-enum-reference",
    test_minimal_golden: "minimal",
    test_multiple_similar_request_schemas_golden: "multiple-similar-request-schemas",
    test_naming_conventions_golden: "naming-conventions",
    test_request_body_content_types_golden: "request-body-content-types",
    test_optional_request_bodies_golden: "optional-request-bodies",
    test_binary_transfer_media_types_golden: "binary-transfer-media-types",
    test_media_type_selection_golden: "media-type-selection",
    test_multipart_explicit_encoding_golden: "multipart-explicit-encoding",
    test_multipart_edge_cases_golden: "multipart-edge-cases",
    test_multipart_nested_object_parts_golden: "multipart-nested-object-parts",
    test_multipart_unsupported_schema_golden: "multipart-unsupported-schema",
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
    test_recursive_json_self_referential_object_golden: "recursive-json-self-referential-object",

    test_type_aliases_complex_union_golden: "type-aliases-complex-union",
    test_type_aliases_intersection_allof_golden: "type-aliases-intersection-allof",
    test_type_aliases_intersection_with_nullable_reference_golden: "type-aliases-intersection-with-nullable-reference",
    test_type_aliases_nested_union_golden: "type-aliases-nested-union",
    test_type_aliases_simple_type_alias_golden: "type-aliases-simple-type-alias",
    test_type_aliases_union_mixed_golden: "type-aliases-union-mixed",
    test_type_aliases_union_with_any_golden: "type-aliases-union-with-any",
    test_type_aliases_union_with_inline_objects_golden: "type-aliases-union-with-inline-objects",
    test_type_aliases_union_with_interfaces_golden: "type-aliases-union-with-interfaces",
    test_type_aliases_union_with_primitives_golden: "type-aliases-union-with-primitives",
    test_type_aliases_discriminated_union_internally_tagged_golden: "type-aliases-discriminated-union-internally-tagged",
    test_type_aliases_discriminated_union_with_refs_golden: "type-aliases-discriminated-union-with-refs",
    test_type_aliases_discriminated_union_multiple_golden: "type-aliases-discriminated-union-multiple",
    test_type_aliases_discriminated_union_long_names_golden: "type-aliases-discriminated-union-long-names",
    test_type_aliases_discriminated_union_inline_discriminator_only_golden: "type-aliases-discriminated-union-inline-discriminator-only",
    test_type_aliases_discriminated_union_mixed_unit_and_allof_golden: "type-aliases-discriminated-union-mixed-unit-and-allof",

    test_response_body_default_and_exact_golden: "response-body-default-and-exact",
    test_response_body_fallback_golden: "response-body-fallback",
    test_response_body_multi_status_responses_golden: "response-body-multi-status-responses",
    test_response_body_no_response_body_golden: "response-body-no-response-body",
    test_typed_error_responses_golden: "typed-error-responses",
    test_response_dispatch_golden: "response-dispatch",
    test_api_interface_golden: "api-interface",

    test_enum_repr_golden: "enum-repr",
    test_additional_properties_golden: "additional-properties",

    test_query_param_enum_golden: "query-param-enum",
    test_server_path_prefix_golden: "server-path-prefix",
}

// ===========================================================================
// Config-aware golden tests (enum const objects + type guards)
// ===========================================================================

#[test]
#[traced_test]
fn test_enum_const_object_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
emit_enum_constants = true
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-enum-const",
        "valid/ts-enum-const.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_type_guards_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
emit_type_guards = true
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-type-guards",
        "valid/ts-type-guards.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case",
        "valid/naming-conventions.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_multipart_parts_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-multipart-parts",
        "valid/multipart-nested-object-parts.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_response_only_types_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-response-only-types",
        "valid/delete-with-response-schema.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_response_dispatch_golden() {
    let config = toml::from_str("property_naming = 'camelCase'").unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-response-dispatch",
        "valid/typescript-response-dispatch.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_tagged_union_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-tagged-union",
        "valid/type-aliases/discriminated-union-with-refs.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_tagged_union_variant_with_discriminator_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-tagged-union-variant-with-discriminator",
        "valid/type-aliases/discriminated-union-variant-with-discriminator-field.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_tagged_union_plain_internal_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-tagged-union-plain-internal",
        "valid/type-aliases/discriminated-union-plain-internal.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_intersection_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-intersection",
        "valid/type-aliases/intersection-allof.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_union_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-union",
        "valid/type-aliases/union-with-interfaces.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_enum_ref_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-enum-ref",
        "valid/interface-with-enum-reference.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_intersection_enum_ref_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-intersection-enum-ref",
        "valid/type-aliases/intersection-with-enum-ref.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_property_naming_camel_case_externally_tagged_golden() {
    let config: toml::value::Table = toml::from_str(
        r#"
property_naming = "camelCase"
"#,
    )
    .unwrap();
    let generator = TypeScriptFetchCodeGenerator::new(config);
    run_golden_test(
        &generator,
        golden_dir(),
        "ts-property-naming-camel-case-externally-tagged",
        "valid/type-aliases/discriminated-union-externally-tagged.yaml",
        UPDATE_HINT,
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
                foo_file
                    .content
                    .contains("readonly children?: readonly Foo$Wire[];"),
                "Foo$Wire.children must use the local wire type:\n{}",
                foo_file.content
            );
            assert!(
                foo_file
                    .content
                    .contains("json.children.map((item) => fooFromJSON(item))"),
                "Foo's decoder must call itself locally:\n{}",
                foo_file.content
            );
            assert!(
                foo_file
                    .content
                    .contains("value.children.map((item) => fooToJSON(item))"),
                "Foo's encoder must call itself locally:\n{}",
                foo_file.content
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

// ===========================================================================
// Toolchain = "vp" golden tests
// Same fixtures re-generated with vp toolchain to verify both paths compile.
// ===========================================================================

macro_rules! generate_vp_golden_tests {
    ($($test_name:ident: ($golden_name:expr, $fixture:expr, $extra_config:expr)),* $(,)?) => {
        $(
            #[test]
            #[traced_test]
            fn $test_name() {
                let config: toml::value::Table = toml::from_str(
                    &format!("toolchain = \"vp\"\n{}", $extra_config),
                )
                .unwrap();
                let generator = TypeScriptFetchCodeGenerator::new(config);
                run_golden_test(
                    &generator,
                    golden_dir(),
                    $golden_name,
                    $fixture,
                    UPDATE_HINT,
                );
            }
        )*
    };
}

generate_vp_golden_tests! {
    test_toolchain_vp_petstore_golden: ("ts-toolchain-vp-petstore", "valid/petstore.yaml", ""),
    test_toolchain_vp_comprehensive_golden: ("ts-toolchain-vp-comprehensive", "valid/comprehensive-schemas.yaml", ""),
    test_toolchain_vp_camel_case_golden: ("ts-toolchain-vp", "valid/type-aliases/discriminated-union-with-refs.yaml", "property_naming = \"camelCase\""),
    test_toolchain_vp_delete_response_golden: ("ts-toolchain-vp-delete-response", "valid/delete-with-response-schema.yaml", "property_naming = \"camelCase\""),
    test_toolchain_vp_enum_repr_golden: ("ts-toolchain-vp-enum-repr", "valid/enum-repr/enum-repr.yaml", "emit_enum_constants = true"),
    test_toolchain_vp_self_referential_object_golden: (
        "ts-toolchain-vp-self-referential-object",
        "valid/recursive-json/self-referential-object.yaml",
        "property_naming = \"camelCase\""
    ),
    test_toolchain_vp_camel_case_tagged_nullable_unions_golden: (
        "ts-toolchain-vp-camel-case-tagged-nullable-unions",
        "valid/type-aliases/typescript-fetch-vp-camel-case-tagged-nullable-unions.yaml",
        "emit_enum_constants = true\nemit_type_guards = true\nproperty_naming = \"camelCase\""
    ),
}
