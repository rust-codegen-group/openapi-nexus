//! Unified schema mapping utilities for OpenAPI to TypeScript conversion

use std::collections::BTreeSet;

use heck::ToPascalCase as _;
use utoipa::openapi;
use utoipa::openapi::{RefOr, Schema};

use crate::ast::{TsExpression, TsPrimitive};

/// Unified schema mapper for converting OpenAPI schemas to TypeScript types
#[derive(Debug, Clone)]
pub struct SchemaMapper;

impl SchemaMapper {
    /// Map a RefOr<Schema> to a TypeScript type expression
    ///
    /// For schema references, this always uses the PascalCase version of the schema name.
    /// The original schema name is preserved in imports with an alias when needed.
    pub fn map_ref_or_schema_to_type(schema_ref: &RefOr<Schema>) -> TsExpression {
        match schema_ref {
            RefOr::T(schema) => Self::map_schema_to_type(schema),
            RefOr::Ref(reference) => {
                let ref_path = &reference.ref_location;
                if let Some(schema_name) = ref_path.strip_prefix("#/components/schemas/") {
                    TsExpression::Reference(schema_name.to_pascal_case())
                } else {
                    TsExpression::Primitive(TsPrimitive::String)
                }
            }
        }
    }

    /// Map a Schema to a TypeScript type expression
    pub fn map_schema_to_type(schema: &Schema) -> TsExpression {
        match schema {
            Schema::Object(obj_schema) => {
                // Check if this is an enum schema
                if let Some(enum_values) = &obj_schema.enum_values
                    && !enum_values.is_empty()
                {
                    return Self::map_enum_to_type(enum_values);
                }

                if obj_schema.properties.is_empty() {
                    // This is likely a primitive type
                    TsExpression::Primitive(TsPrimitive::String)
                } else {
                    TsExpression::Reference("object".to_string())
                }
            }
            Schema::Array(arr_schema) => {
                // Map array schema to TypeScript array type using the items field
                let item_type = Self::map_array_items_to_type(&arr_schema.items);
                TsExpression::Array(Box::new(item_type))
            }
            _ => TsExpression::Primitive(TsPrimitive::String),
        }
    }

    /// Map enum values to TypeScript type
    fn map_enum_to_type(enum_values: &[serde_json::Value]) -> TsExpression {
        let mut types = Vec::new();
        for enum_value in enum_values {
            match enum_value {
                serde_json::Value::String(s) => {
                    types.push(TsExpression::Literal(format!("\"{}\"", s)));
                }
                serde_json::Value::Number(n) => {
                    types.push(TsExpression::Literal(n.to_string()));
                }
                serde_json::Value::Bool(b) => {
                    types.push(TsExpression::Literal(b.to_string()));
                }
                _ => {
                    types.push(TsExpression::Literal(enum_value.to_string()));
                }
            }
        }

        match types.len() {
            0 => TsExpression::Primitive(TsPrimitive::Any),
            1 => types
                .into_iter()
                .next()
                .expect("types should have exactly one element"),
            _ => {
                let unique_types: BTreeSet<TsExpression> = types.into_iter().collect();
                match unique_types.len() {
                    1 => unique_types
                        .into_iter()
                        .next()
                        .expect("unique_types should have exactly one element"),
                    _ => TsExpression::Union(unique_types),
                }
            }
        }
    }

    /// Map ArrayItems to TypeScript type
    fn map_array_items_to_type(array_items: &openapi::schema::ArrayItems) -> TsExpression {
        match array_items {
            openapi::schema::ArrayItems::RefOrSchema(schema_ref) => {
                Self::map_ref_or_schema_to_type(schema_ref)
            }
            openapi::schema::ArrayItems::False => {
                // No additional items allowed - use any as fallback
                TsExpression::Primitive(TsPrimitive::Any)
            }
        }
    }
}
