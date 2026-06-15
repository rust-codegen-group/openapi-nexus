//! Golden file tests for Rust aioduct generator
//!
//! These tests compare generated Rust code against known-good golden files.
//! To update golden files after intentional changes, run:
//!   UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_aioduct

use std::collections::HashMap;
use std::path::Path;

use tracing_test::traced_test;

use openapi_nexus::generators::rust::aioduct::RustAioductCodeGenerator;
use openapi_nexus::test_utils::{run_golden_test, test_cases_from_slice};

fn golden_dir() -> &'static Path {
    Path::new("tests/golden/rust/rust-aioduct")
}

const UPDATE_HINT: &str = "UPDATE_GOLDEN=1 cargo test --test golden_tests_rust_aioduct";

#[rustfmt::skip]
fn get_golden_test_cases() -> HashMap<&'static str, &'static str> {
    test_cases_from_slice(&[
        ("petstore", "valid/petstore.yaml"),
        ("minimal", "valid/minimal.yaml"),
        ("comprehensive-schemas", "valid/comprehensive-schemas.yaml"),
        ("enum-repr", "valid/enum-repr/enum-repr.yaml"),
        ("additional-properties", "valid/additional-properties/additional-properties.yaml"),
        ("naming-conventions", "valid/naming-conventions.yaml"),
        ("server-object", "valid/server-object.yaml"),
        ("delete-with-response-schema", "valid/delete-with-response-schema.yaml"),
        ("duplicate-param-names", "valid/duplicate-param-names.yaml"),
        ("interface-with-enum-reference", "valid/interface-with-enum-reference.yaml"),
        ("multiple-similar-request-schemas", "valid/multiple-similar-request-schemas.yaml"),
        ("request-body-content-types", "valid/request-body-content-types.yaml"),
        ("binary-transfer-media-types", "valid/binary-transfer-media-types.yaml"),
        ("media-type-selection", "valid/media-type-selection.yaml"),
        ("multipart-edge-cases", "valid/multipart-edge-cases.yaml"),
        ("multipart-nested-object-parts", "valid/multipart-nested-object-parts.yaml"),
        ("multipart-unsupported-schema", "valid/multipart-unsupported-schema.yaml"),

        ("type-aliases-simple-type-alias", "valid/type-aliases/simple-type-alias.yaml"),
        ("type-aliases-complex-union", "valid/type-aliases/complex-union.yaml"),
        ("type-aliases-intersection-allof", "valid/type-aliases/intersection-allof.yaml"),
        ("type-aliases-nested-union", "valid/type-aliases/nested-union.yaml"),
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
        ("type-aliases-intersection-with-nullable-reference", "valid/type-aliases/intersection-with-nullable-reference.yaml"),

        ("response-body-default-and-exact", "valid/response-body/default-and-exact.yaml"),
        ("response-body-fallback", "valid/response-body/fallback.yaml"),
        ("response-body-multi-status-responses", "valid/response-body/multi-status-responses.yaml"),
        ("response-body-no-response-body", "valid/response-body/no-response-body.yaml"),

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

        ("query-param-enum", "valid/query/query-param-enum.yaml"),
        ("server-path-prefix", "valid/server-path-prefix.yaml"),
        ("multiline-docs-and-primitive-alias", "valid/multiline-docs-and-primitive-alias.yaml"),
    ])
}

macro_rules! generate_golden_tests {
    ($($test_name:ident: $spec_name:expr),* $(,)?) => {
        $(
            #[test]
            #[traced_test]
            fn $test_name() {
                let cases = get_golden_test_cases();
                let generator = RustAioductCodeGenerator::new(toml::value::Table::new());
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
    test_petstore_golden: "petstore",
    test_minimal_golden: "minimal",
    test_comprehensive_schemas_golden: "comprehensive-schemas",
    test_enum_repr_golden: "enum-repr",
    test_additional_properties_golden: "additional-properties",
    test_naming_conventions_golden: "naming-conventions",
    test_server_object_golden: "server-object",
    test_delete_with_response_schema_golden: "delete-with-response-schema",
    test_duplicate_param_names_golden: "duplicate-param-names",
    test_interface_with_enum_reference_golden: "interface-with-enum-reference",
    test_multiple_similar_request_schemas_golden: "multiple-similar-request-schemas",
    test_request_body_content_types_golden: "request-body-content-types",
    test_binary_transfer_media_types_golden: "binary-transfer-media-types",
    test_media_type_selection_golden: "media-type-selection",
    test_multipart_edge_cases_golden: "multipart-edge-cases",
    test_multipart_nested_object_parts_golden: "multipart-nested-object-parts",
    test_multipart_unsupported_schema_golden: "multipart-unsupported-schema",

    test_type_aliases_simple_type_alias_golden: "type-aliases-simple-type-alias",
    test_type_aliases_complex_union_golden: "type-aliases-complex-union",
    test_type_aliases_intersection_allof_golden: "type-aliases-intersection-allof",
    test_type_aliases_nested_union_golden: "type-aliases-nested-union",
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
    test_type_aliases_intersection_with_nullable_reference_golden: "type-aliases-intersection-with-nullable-reference",

    test_response_body_default_and_exact_golden: "response-body-default-and-exact",
    test_response_body_fallback_golden: "response-body-fallback",
    test_response_body_multi_status_responses_golden: "response-body-multi-status-responses",
    test_response_body_no_response_body_golden: "response-body-no-response-body",

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

    test_query_param_enum_golden: "query-param-enum",
    test_server_path_prefix_golden: "server-path-prefix",
    test_multiline_docs_and_primitive_alias_golden: "multiline-docs-and-primitive-alias",
}

// --- Utoipa golden tests (require custom config) ---

fn utoipa_config() -> toml::value::Table {
    toml::from_str::<toml::value::Table>(
        r#"
        [utoipa]
        enabled = true
        dependency = '{ version = "5" }'
        "#,
    )
    .unwrap()
}

fn utoipa_with_extra_derives_config() -> toml::value::Table {
    toml::from_str::<toml::value::Table>(
        r#"
        [utoipa]
        enabled = true
        dependency = '{ version = "5" }'

        [extra_derives.structs]
        derives = ["PartialEq", "Eq"]

        [extra_derives.enums]
        derives = ["PartialEq", "Eq", "Hash"]
        "#,
    )
    .unwrap()
}

#[test]
#[traced_test]
fn test_utoipa_mixed_golden() {
    let generator = RustAioductCodeGenerator::new(utoipa_config());
    run_golden_test(
        &generator,
        golden_dir(),
        "utoipa-mixed",
        "valid/utoipa/utoipa-mixed.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_utoipa_untagged_union_golden() {
    let generator = RustAioductCodeGenerator::new(utoipa_config());
    run_golden_test(
        &generator,
        golden_dir(),
        "utoipa-untagged-union",
        "valid/utoipa/utoipa-untagged-union.yaml",
        UPDATE_HINT,
    );
}

#[test]
#[traced_test]
fn test_utoipa_with_extra_derives_golden() {
    let generator = RustAioductCodeGenerator::new(utoipa_with_extra_derives_config());
    run_golden_test(
        &generator,
        golden_dir(),
        "utoipa-with-extra-derives",
        "valid/utoipa/utoipa-with-extra-derives.yaml",
        UPDATE_HINT,
    );
}
