//! Validate references in owned inline schema nodes without resolving named edges.

use super::LowerError;

/// Version adapters expose only the child slots already consumed by lowering.
/// Boolean schemas have no children and local references remain terminal edges.
pub(super) trait InlineSchema: Sized {
    type Member;

    fn for_each_child(
        &self,
        visitor: impl FnMut(&Self::Member) -> Result<(), LowerError>,
    ) -> Result<(), LowerError>;
    fn reference(member: &Self::Member) -> Option<&str>;
    fn object(member: &Self::Member) -> Option<&Self>;
}

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

/// Apply the existing HTTP(S) reference policy to the semantic children that
/// lowering will consume in an owned inline schema. Do not follow local
/// references, inspect metadata, or reinterpret ignored schema keywords.
pub(super) fn validate_inline_references<S: InlineSchema>(schema: &S) -> Result<(), LowerError> {
    schema.for_each_child(|member| {
        if let Some(reference) = S::reference(member) {
            validate_reference(reference)?;
        }
        if let Some(object) = S::object(member) {
            validate_inline_references(object)?;
        }
        Ok(())
    })
}

macro_rules! adapt_schema {
    ($version:ident, $schema:ident => $additional:expr, $object_branch:expr, $array_branch:expr) => {
        impl InlineSchema for crate::spec::$version::spec::ObjectSchema {
            type Member = crate::spec::$version::spec::ObjectOrReference<Self>;

            fn for_each_child(
                &self,
                mut visitor: impl FnMut(&Self::Member) -> Result<(), LowerError>,
            ) -> Result<(), LowerError> {
                use crate::spec::$version::spec::Schema;

                // Match the lowering classifier's precedence: enum values and
                // compositions consume the schema before container siblings.
                if !self.enum_values.is_empty() {
                    return Ok(());
                }
                let composition = if !self.one_of.is_empty() {
                    Some(&self.one_of)
                } else if !self.any_of.is_empty() {
                    Some(&self.any_of)
                } else if !self.all_of.is_empty() {
                    Some(&self.all_of)
                } else {
                    None
                };
                if let Some(members) = composition {
                    for member in members {
                        visitor(member)?;
                    }
                    return Ok(());
                }
                let $schema = self;
                let additional: Option<&Schema> = $additional;
                let object_branch = $object_branch;
                let array_branch = $array_branch;
                if object_branch {
                    for member in self.properties.values() {
                        visitor(member)?;
                    }
                    if let Some(Schema::Object(member)) = additional {
                        visitor(member)?;
                    }
                } else if array_branch {
                    if let Some(Schema::Object(member)) = self.items.as_deref() {
                        visitor(member)?;
                    }
                }
                Ok(())
            }

            fn reference(member: &Self::Member) -> Option<&str> {
                match member {
                    Self::Member::Ref { ref_path, .. } => Some(ref_path.as_str()),
                    Self::Member::Object(_) => None,
                }
            }

            fn object(member: &Self::Member) -> Option<&Self> {
                match member {
                    Self::Member::Object(object) => Some(object),
                    Self::Member::Ref { .. } => None,
                }
            }
        }
    };
}

adapt_schema!(
    oas30,
    schema => match &schema.additional_properties {
        Some(crate::spec::oas30::spec::AdditionalProperties::Schema(value)) => Some(value.as_ref()),
        _ => None,
    },
    schema
        .schema_type
        .as_deref()
        .is_none_or(|kind| kind == "object")
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema.schema_type.as_deref() == Some("array")
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);
adapt_schema!(
    oas31,
    schema => schema.additional_properties.as_ref(),
    schema
        .schema_type
        .as_ref()
        .is_none_or(|types| types.contains(crate::spec::oas31::spec::SchemaType::Object))
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema
        .schema_type
        .as_ref()
        .is_some_and(|types| types.contains(crate::spec::oas31::spec::SchemaType::Array))
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);
adapt_schema!(
    oas32,
    schema => schema.additional_properties.as_ref(),
    schema
        .schema_type
        .as_ref()
        .is_none_or(|types| types.contains(crate::spec::oas32::spec::SchemaType::Object))
        && (!schema.properties.is_empty() || schema.additional_properties.is_some()),
    schema
        .schema_type
        .as_ref()
        .is_some_and(|types| types.contains(crate::spec::oas32::spec::SchemaType::Array))
        || (schema.schema_type.is_none()
            && schema.properties.is_empty()
            && schema.additional_properties.is_none()
            && schema.items.is_some())
);

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::{InlineSchema, LowerError, validate_inline_references};

    fn assert_version<S: InlineSchema + DeserializeOwned>(source: &Value, expected: Option<&str>) {
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
}
