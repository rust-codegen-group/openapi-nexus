//! Type mapping from OpenAPI types to Rust types

use crate::ast::{PrimitiveType, TypeExpression};
use utoipa::openapi::Schema;

/// Comprehensive type mapping from OpenAPI to Rust
pub struct TypeMapper;

impl TypeMapper {
    /// Map OpenAPI schema to Rust type expression
    pub fn map_schema_to_rust_type(&self, schema: &Schema) -> TypeExpression {
        // Handle different OpenAPI schema types
        match self.get_schema_type(schema) {
            OpenApiType::String => self.map_string_type(schema),
            OpenApiType::Integer => self.map_integer_type(schema),
            OpenApiType::Number => self.map_number_type(schema),
            OpenApiType::Boolean => TypeExpression::Primitive(PrimitiveType::Bool),
            OpenApiType::Array => self.map_array_type(schema),
            OpenApiType::Object => self.map_object_type(schema),
            OpenApiType::Enum => self.map_enum_type(schema),
            OpenApiType::OneOf => self.map_one_of_type(schema),
            OpenApiType::AnyOf => self.map_any_of_type(schema),
            OpenApiType::AllOf => self.map_all_of_type(schema),
            OpenApiType::Reference => self.map_reference_type(schema),
            OpenApiType::Unknown => TypeExpression::Reference("serde_json::Value".to_string()),
        }
    }

    fn get_schema_type(&self, _schema: &Schema) -> OpenApiType {
        // TODO: Implement proper schema type detection based on utoipa Schema structure
        // For now, return Unknown as we need to understand the actual schema fields
        OpenApiType::Unknown
    }

    fn map_string_type(&self, schema: &Schema) -> TypeExpression {
        // Handle string formats
        if let Some(format) = self.get_string_format(schema) {
            match format {
                StringFormat::DateTime => {
                    TypeExpression::Reference("chrono::DateTime<chrono::Utc>".to_string())
                }
                StringFormat::Date => TypeExpression::Reference("chrono::NaiveDate".to_string()),
                StringFormat::Time => TypeExpression::Reference("chrono::NaiveTime".to_string()),
                StringFormat::Uuid => TypeExpression::Reference("uuid::Uuid".to_string()),
                StringFormat::Email => TypeExpression::Primitive(PrimitiveType::String),
                StringFormat::Uri => TypeExpression::Reference("url::Url".to_string()),
                StringFormat::Binary => TypeExpression::Reference("Vec<u8>".to_string()),
                StringFormat::Password => TypeExpression::Primitive(PrimitiveType::String),
                StringFormat::Other(_) => TypeExpression::Primitive(PrimitiveType::String),
            }
        } else {
            TypeExpression::Primitive(PrimitiveType::String)
        }
    }

    fn map_integer_type(&self, schema: &Schema) -> TypeExpression {
        // Handle integer formats
        if let Some(format) = self.get_integer_format(schema) {
            match format {
                IntegerFormat::Int32 => TypeExpression::Primitive(PrimitiveType::I32),
                IntegerFormat::Int64 => TypeExpression::Primitive(PrimitiveType::I64),
                IntegerFormat::Other(_) => TypeExpression::Primitive(PrimitiveType::I32),
            }
        } else {
            TypeExpression::Primitive(PrimitiveType::I32)
        }
    }

    fn map_number_type(&self, schema: &Schema) -> TypeExpression {
        // Handle number formats
        if let Some(format) = self.get_number_format(schema) {
            match format {
                NumberFormat::Float => TypeExpression::Primitive(PrimitiveType::F32),
                NumberFormat::Double => TypeExpression::Primitive(PrimitiveType::F64),
                NumberFormat::Other(_) => TypeExpression::Primitive(PrimitiveType::F64),
            }
        } else {
            TypeExpression::Primitive(PrimitiveType::F64)
        }
    }

    fn map_array_type(&self, schema: &Schema) -> TypeExpression {
        // Handle array item types
        if let Some(items_schema) = self.get_array_items(schema) {
            let item_type = self.map_schema_to_rust_type(items_schema);
            TypeExpression::Vec(Box::new(item_type))
        } else {
            TypeExpression::Vec(Box::new(TypeExpression::Primitive(PrimitiveType::String)))
        }
    }

    fn map_object_type(&self, _schema: &Schema) -> TypeExpression {
        // For generic objects, use serde_json::Value
        // TODO: Generate specific struct types for objects with defined properties
        TypeExpression::Reference("serde_json::Value".to_string())
    }

    fn map_enum_type(&self, _schema: &Schema) -> TypeExpression {
        // TODO: Generate enum types for string enums
        TypeExpression::Primitive(PrimitiveType::String)
    }

    fn map_one_of_type(&self, _schema: &Schema) -> TypeExpression {
        // TODO: Handle oneOf with union types
        TypeExpression::Reference("serde_json::Value".to_string())
    }

    fn map_any_of_type(&self, _schema: &Schema) -> TypeExpression {
        // TODO: Handle anyOf with union types
        TypeExpression::Reference("serde_json::Value".to_string())
    }

    fn map_all_of_type(&self, _schema: &Schema) -> TypeExpression {
        // TODO: Handle allOf with intersection types
        TypeExpression::Reference("serde_json::Value".to_string())
    }

    fn map_reference_type(&self, _schema: &Schema) -> TypeExpression {
        // TODO: Resolve references to actual types
        TypeExpression::Reference("serde_json::Value".to_string())
    }

    // Helper methods for extracting schema information
    fn get_string_format(&self, _schema: &Schema) -> Option<StringFormat> {
        // TODO: Extract string format from schema
        None
    }

    fn get_integer_format(&self, _schema: &Schema) -> Option<IntegerFormat> {
        // TODO: Extract integer format from schema
        None
    }

    fn get_number_format(&self, _schema: &Schema) -> Option<NumberFormat> {
        // TODO: Extract number format from schema
        None
    }

    fn get_array_items(&self, _schema: &Schema) -> Option<&Schema> {
        // TODO: Extract array items schema
        None
    }
}

/// OpenAPI schema types
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum OpenApiType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
    Enum,
    OneOf,
    AnyOf,
    AllOf,
    Reference,
    Unknown,
}

/// String format types
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum StringFormat {
    DateTime,
    Date,
    Time,
    Uuid,
    Email,
    Uri,
    Binary,
    Password,
    Other(String),
}

/// Integer format types
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum IntegerFormat {
    Int32,
    Int64,
    Other(String),
}

/// Number format types
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum NumberFormat {
    Float,
    Double,
    Other(String),
}
