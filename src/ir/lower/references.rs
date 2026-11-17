//! Validate references in owned inline schema nodes without resolving named edges.

use super::LowerError;
use crate::ir::schema_edges::{SchemaEdgeMode, SchemaEdges};

/// Apply the existing case-sensitive HTTP(S) reference policy at every direct
/// reference seam. Local and non-HTTP references remain named edges.
pub(super) fn validate_reference(reference: &str) -> Result<(), LowerError> {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        Err(LowerError::ExternalReference {
            reference: reference.to_owned(),
        })
    } else {
        Ok(())
    }
}

/// Apply the HTTP(S) reference policy to the semantic children that lowering
/// consumes in an owned inline schema. The raw edge adapter keeps version
/// details and precedence in one place; this function owns only policy.
pub(super) fn validate_inline_references<S: SchemaEdges>(schema: &S) -> Result<(), LowerError> {
    schema.visit_edges(SchemaEdgeMode::Consumed, |member| {
        if let Some(reference) = S::reference(member) {
            validate_reference(reference)?;
        }
        if let Some(object) = S::object(member) {
            validate_inline_references(object)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::{LowerError, validate_inline_references};
    use crate::ir::schema_edges::SchemaEdges;

    fn assert_version<S: SchemaEdges + DeserializeOwned>(source: &Value, expected: Option<&str>) {
        let schema: S = serde_json::from_value(source.clone()).expect("inline schema parses");
        let actual = match validate_inline_references(&schema) {
            Ok(()) => None,
            Err(LowerError::ExternalReference { reference }) => Some(reference),
            Err(error) => panic!("unexpected inline reference error: {error}"),
        };
        assert_eq!(actual.as_deref(), expected, "{source}");
    }

    fn assert_every_version(source: Value, expected: Option<&str>) {
        assert_version::<crate::spec::oas30::spec::ObjectSchema>(&source, expected);
        assert_version::<crate::spec::oas31::spec::ObjectSchema>(&source, expected);
        assert_version::<crate::spec::oas32::spec::ObjectSchema>(&source, expected);
    }

    fn assert_v31_v32(source: Value, expected: Option<&str>) {
        assert_version::<crate::spec::oas31::spec::ObjectSchema>(&source, expected);
        assert_version::<crate::spec::oas32::spec::ObjectSchema>(&source, expected);
    }
    fn owned_containers(child: Value) -> [Value; 6] {
        [
            json!({"properties": {"value": child.clone()}}),
            json!({"oneOf": [child.clone()]}),
            json!({"anyOf": [child.clone()]}),
            json!({"allOf": [child.clone()]}),
            json!({"type": "array", "items": child.clone()}),
            json!({"type": "object", "additionalProperties": child}),
        ]
    }

    #[test]
    fn rejects_http_references_in_each_supported_owned_child_slot() {
        for reference in [
            "http://example.invalid/schema",
            "https://example.invalid/schema",
        ] {
            for schema in owned_containers(json!({"$ref": reference})) {
                assert_every_version(schema, Some(reference));
            }
        }
    }

    #[test]
    fn descends_through_nested_container_and_composition_nodes() {
        let reference = "https://example.invalid/schema";
        for inner in owned_containers(json!({"$ref": reference})) {
            for outer in owned_containers(inner) {
                assert_every_version(outer, Some(reference));
            }
        }
        assert_every_version(
            json!({
                "properties": {
                    "values": {
                        "oneOf": [{
                            "type": "array",
                            "items": {
                                "allOf": [{
                                    "type": "object",
                                    "additionalProperties": {
                                        "anyOf": [{"$ref": reference}, {"type": "string"}]
                                    }
                                }]
                            }
                        }]
                    }
                }
            }),
            Some(reference),
        );
    }

    #[test]
    fn boolean_schemas_and_local_recursive_edges_are_terminal() {
        for boolean in [false, true] {
            assert_every_version(
                json!({
                    "properties": {"child": {"$ref": "#/components/schemas/Node"}},
                    "oneOf": [{"$ref": "#/components/schemas/Node"}],
                    "anyOf": [{"$ref": "#/components/schemas/Node"}],
                    "allOf": [{"$ref": "#/components/schemas/Node"}],
                    "items": boolean,
                    "additionalProperties": boolean
                }),
                None,
            );
        }
    }

    #[test]
    fn preserves_the_existing_reference_scheme_boundary() {
        for reference in [
            "#/components/schemas/Node",
            "relative.yaml#/Node",
            "file:///schema",
            "urn:schema:node",
            "HTTP://example.invalid/schema",
        ] {
            for schema in owned_containers(json!({"$ref": reference})) {
                assert_every_version(schema, None);
            }
        }
        assert_every_version(
            json!({
                "type": "object",
                "example": {"$ref": "https://example.invalid/example"},
                "x-metadata": {"$ref": "https://example.invalid/metadata"}
            }),
            None,
        );
    }

    #[test]
    fn reports_the_first_owned_reference_deterministically() {
        assert_every_version(
            json!({
                "properties": {
                    "z": {"$ref": "https://example.invalid/later"},
                    "a": {"$ref": "https://example.invalid/first"}
                }
            }),
            Some("https://example.invalid/first"),
        );
    }

    #[test]
    fn mixed_object_and_array_type_sets_visit_both_consumed_edges() {
        assert_v31_v32(
            json!({
                "type": ["object", "array"],
                "additionalProperties": {"$ref": "https://example.invalid/map"},
                "items": {"$ref": "https://example.invalid/array"}
            }),
            Some("https://example.invalid/map"),
        );
        assert_v31_v32(
            json!({
                "type": ["object", "array"],
                "additionalProperties": {"$ref": "#/components/schemas/Map"},
                "items": {"$ref": "https://example.invalid/array"}
            }),
            Some("https://example.invalid/array"),
        );
    }
}
