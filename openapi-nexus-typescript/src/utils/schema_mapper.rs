//! Unified schema mapping utilities for OpenAPI to TypeScript conversion

use std::collections::BTreeSet;

use heck::ToPascalCase as _;

use crate::ast::{TsExpression, TsPrimitive};
use openapi_nexus_spec::oas31::spec::{ObjectOrReference, ObjectSchema, Schema};

/// Unified schema mapper for converting OpenAPI schemas to TypeScript types
#[derive(Debug, Clone)]
pub struct SchemaMapper;

impl SchemaMapper {
    /// Map an ObjectOrReference<ObjectSchema> to a TypeScript type expression
    ///
    /// For schema references, this always uses the PascalCase version of the schema name.
    /// The original schema name is preserved in imports with an alias when needed.
    pub fn map_ref_or_schema_to_type(schema_ref: &ObjectOrReference<ObjectSchema>) -> TsExpression {
        match schema_ref {
            ObjectOrReference::Object(object_schema) => {
                Self::map_object_schema_to_type(object_schema)
            }
            ObjectOrReference::Ref { ref_path, .. } => {
                if let Some(schema_name) = ref_path.strip_prefix("#/components/schemas/") {
                    TsExpression::Reference(schema_name.to_pascal_case())
                } else {
                    TsExpression::Primitive(TsPrimitive::String)
                }
            }
        }
    }

    /// Map an object schema to a TypeScript type expression
    fn map_object_schema_to_type(object_schema: &ObjectSchema) -> TsExpression {
        if !object_schema.enum_values.is_empty() {
            return Self::map_enum_to_type(&object_schema.enum_values);
        }
        if let Some(items) = &object_schema.items {
            let item_type = Self::map_schema_to_ts_type(items);
            return TsExpression::Array(Box::new(item_type));
        }
        if object_schema.properties.is_empty() {
            TsExpression::Primitive(TsPrimitive::String)
        } else {
            TsExpression::Reference("object".to_string())
        }
    }

    /// Map schema (Boolean | Object) to TsExpression
    fn map_schema_to_ts_type(schema: &Schema) -> TsExpression {
        match schema {
            Schema::Object(schema_ref) => Self::map_ref_or_schema_to_type(schema_ref.as_ref()),
            Schema::Boolean(_) => TsExpression::Primitive(TsPrimitive::Any),
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
}
