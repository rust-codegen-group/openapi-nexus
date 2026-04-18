//! Predicate for deciding whether an inline schema should be promoted to a named ref.

use openapi_nexus_spec::oas31::spec::{ObjectOrReference, ObjectSchema, Schema};

/// Returns `true` if the given inline `ObjectSchema` is structurally complex
/// enough to warrant promotion to a named entry in `components/schemas`.
///
/// Promotes:
/// - Objects with non-empty `properties`
/// - Composition schemas (allOf/anyOf/oneOf) that contain at least one inline member
///   which itself should be promoted
/// - Arrays whose `items` is an inline schema that should be promoted
/// - Schemas whose `additionalProperties` is an inline schema that should be promoted
///
/// Does NOT promote:
/// - Simple primitives (string, integer, number, boolean)
/// - Enums without properties
/// - Empty objects (type: object with no properties)
pub fn should_promote(schema: &ObjectSchema) -> bool {
    // Objects with properties are always worth naming
    if !schema.properties.is_empty() {
        return true;
    }

    // Composition schemas with promotable inline members
    if has_promotable_composition_members(schema) {
        return true;
    }

    // Array with promotable inline items
    if has_promotable_items(schema) {
        return true;
    }

    // additionalProperties with promotable inline schema
    if has_promotable_additional_properties(schema) {
        return true;
    }

    false
}

fn has_promotable_composition_members(schema: &ObjectSchema) -> bool {
    [&schema.all_of, &schema.any_of, &schema.one_of]
        .iter()
        .any(|members| {
            members.iter().any(|m| match m {
                ObjectOrReference::Object(obj) => should_promote(obj),
                ObjectOrReference::Ref { .. } => false,
            })
        })
}

fn has_promotable_items(schema: &ObjectSchema) -> bool {
    let Some(items) = &schema.items else {
        return false;
    };
    match items.as_ref() {
        Schema::Object(boxed) => match boxed.as_ref() {
            ObjectOrReference::Object(obj) => should_promote(obj),
            ObjectOrReference::Ref { .. } => false,
        },
        Schema::Boolean(_) => false,
    }
}

fn has_promotable_additional_properties(schema: &ObjectSchema) -> bool {
    let Some(ap) = &schema.additional_properties else {
        return false;
    };
    match ap {
        Schema::Object(boxed) => match boxed.as_ref() {
            ObjectOrReference::Object(obj) => should_promote(obj),
            ObjectOrReference::Ref { .. } => false,
        },
        Schema::Boolean(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use openapi_nexus_spec::oas31::spec::{
        ObjectOrReference, ObjectSchema, SchemaType, SchemaTypeSet,
    };

    use super::*;

    fn primitive_string() -> ObjectSchema {
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::String)),
            ..Default::default()
        }
    }

    fn enum_schema() -> ObjectSchema {
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::String)),
            enum_values: vec![
                serde_json::Value::String("a".into()),
                serde_json::Value::String("b".into()),
            ],
            ..Default::default()
        }
    }

    fn empty_object() -> ObjectSchema {
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            ..Default::default()
        }
    }

    fn object_with_properties() -> ObjectSchema {
        let mut props = BTreeMap::new();
        props.insert(
            "name".to_string(),
            ObjectOrReference::Object(primitive_string()),
        );
        ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Object)),
            properties: props,
            ..Default::default()
        }
    }

    #[test]
    fn test_primitive_not_promoted() {
        assert!(!should_promote(&primitive_string()));
    }

    #[test]
    fn test_enum_not_promoted() {
        assert!(!should_promote(&enum_schema()));
    }

    #[test]
    fn test_empty_object_not_promoted() {
        assert!(!should_promote(&empty_object()));
    }

    #[test]
    fn test_object_with_properties_promoted() {
        assert!(should_promote(&object_with_properties()));
    }

    #[test]
    fn test_one_of_with_primitives_not_promoted() {
        let schema = ObjectSchema {
            one_of: vec![
                ObjectOrReference::Object(primitive_string()),
                ObjectOrReference::Object(ObjectSchema {
                    schema_type: Some(SchemaTypeSet::Single(SchemaType::Integer)),
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
        assert!(!should_promote(&schema));
    }

    #[test]
    fn test_one_of_with_inline_object_promoted() {
        let schema = ObjectSchema {
            one_of: vec![
                ObjectOrReference::Object(object_with_properties()),
                ObjectOrReference::Ref {
                    ref_path: "#/components/schemas/Foo".to_string(),
                    summary: None,
                    description: None,
                },
            ],
            ..Default::default()
        };
        assert!(should_promote(&schema));
    }

    #[test]
    fn test_one_of_with_only_refs_not_promoted() {
        let schema = ObjectSchema {
            one_of: vec![
                ObjectOrReference::Ref {
                    ref_path: "#/components/schemas/Foo".to_string(),
                    summary: None,
                    description: None,
                },
                ObjectOrReference::Ref {
                    ref_path: "#/components/schemas/Bar".to_string(),
                    summary: None,
                    description: None,
                },
            ],
            ..Default::default()
        };
        assert!(!should_promote(&schema));
    }

    #[test]
    fn test_array_with_inline_object_items_promoted() {
        use openapi_nexus_spec::oas31::spec::Schema;
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Array)),
            items: Some(Box::new(Schema::Object(Box::new(
                ObjectOrReference::Object(object_with_properties()),
            )))),
            ..Default::default()
        };
        assert!(should_promote(&schema));
    }

    #[test]
    fn test_array_with_primitive_items_not_promoted() {
        use openapi_nexus_spec::oas31::spec::Schema;
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Array)),
            items: Some(Box::new(Schema::Object(Box::new(
                ObjectOrReference::Object(primitive_string()),
            )))),
            ..Default::default()
        };
        assert!(!should_promote(&schema));
    }

    #[test]
    fn test_array_with_ref_items_not_promoted() {
        use openapi_nexus_spec::oas31::spec::Schema;
        let schema = ObjectSchema {
            schema_type: Some(SchemaTypeSet::Single(SchemaType::Array)),
            items: Some(Box::new(Schema::Object(Box::new(ObjectOrReference::Ref {
                ref_path: "#/components/schemas/Foo".to_string(),
                summary: None,
                description: None,
            })))),
            ..Default::default()
        };
        assert!(!should_promote(&schema));
    }

    #[test]
    fn test_additional_properties_with_inline_object() {
        use openapi_nexus_spec::oas31::spec::Schema;
        let schema = ObjectSchema {
            additional_properties: Some(Schema::Object(Box::new(ObjectOrReference::Object(
                object_with_properties(),
            )))),
            ..Default::default()
        };
        assert!(should_promote(&schema));
    }

    #[test]
    fn test_additional_properties_boolean_not_promoted() {
        use openapi_nexus_spec::oas31::spec::{BooleanSchema, Schema};
        let schema = ObjectSchema {
            additional_properties: Some(Schema::Boolean(BooleanSchema(true))),
            ..Default::default()
        };
        assert!(!should_promote(&schema));
    }
}
