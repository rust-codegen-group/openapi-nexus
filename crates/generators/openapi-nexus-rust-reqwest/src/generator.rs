//! Rust code generator

use heck::ToPascalCase;
use snafu::prelude::*;
use utoipa::openapi::{OpenApi, RefOr, Schema};

use crate::ast::*;
use crate::emitter::RustEmitter;

/// Error type for Rust generation
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
    #[snafu(display("Generator error: {}", message))]
    Generic { message: String },
}

/// Rust code generator
pub struct RustGenerator {
    emitter: RustEmitter,
}

impl Default for RustGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl RustGenerator {
    /// Create a new Rust generator
    pub fn new() -> Self {
        Self {
            emitter: RustEmitter,
        }
    }

    /// Generate Rust code from OpenAPI specification
    pub fn generate(&self, openapi: &OpenApi) -> Result<String, GeneratorError> {
        let mut nodes = Vec::new();

        // Generate structs and enums from schemas
        if let Some(components) = &openapi.components {
            for (name, schema_ref) in &components.schemas {
                if let Some(schema) = self.extract_schema(schema_ref) {
                    match self.schema_to_rust_node(name, schema) {
                        Ok(node) => nodes.push(node),
                        Err(e) => {
                            tracing::warn!("Failed to convert schema {}: {}", name, e);
                        }
                    }
                }
            }
        }

        // Generate API client struct
        nodes.push(self.generate_api_client());

        // Emit the code
        self.emitter
            .emit(&nodes)
            .map_err(|e| GeneratorError::Generic {
                message: e.to_string(),
            })
    }

    fn extract_schema<'a>(&self, schema_ref: &'a RefOr<Schema>) -> Option<&'a Schema> {
        match schema_ref {
            RefOr::T(schema) => Some(schema),
            RefOr::Ref(_) => None, // TODO: Handle references
        }
    }

    fn schema_to_rust_node(
        &self,
        name: &str,
        _schema: &Schema,
    ) -> Result<RustNode, GeneratorError> {
        let rust_name = name.to_pascal_case();

        // TODO: Implement proper schema-to-Rust conversion based on actual utoipa Schema fields

        // For now, generate a simple struct as a placeholder
        // TODO: Implement proper schema-to-Rust conversion based on actual utoipa Schema fields
        let struct_def = Struct {
            name: rust_name,
            fields: vec![
                Field {
                    name: "id".to_string(),
                    type_expr: TypeExpression::Primitive(PrimitiveType::I64),
                    optional: false,
                    visibility: Visibility::Public,
                    documentation: Some("Unique identifier".to_string()),
                },
                Field {
                    name: "name".to_string(),
                    type_expr: TypeExpression::Option(Box::new(TypeExpression::Primitive(
                        PrimitiveType::String,
                    ))),
                    optional: true,
                    visibility: Visibility::Public,
                    documentation: Some("Display name".to_string()),
                },
            ],
            derives: vec![
                "Debug".to_string(),
                "Clone".to_string(),
                "Serialize".to_string(),
                "Deserialize".to_string(),
            ],
            generics: Vec::new(),
            documentation: Some(format!("Generated from OpenAPI schema: {}", name)),
            visibility: Visibility::Public,
        };

        Ok(RustNode::Struct(struct_def))
    }

    fn generate_api_client(&self) -> RustNode {
        let struct_def = Struct {
            name: "ApiClient".to_string(),
            fields: vec![Field {
                name: "base_url".to_string(),
                type_expr: TypeExpression::Primitive(PrimitiveType::String),
                optional: false,
                visibility: Visibility::Private,
                documentation: Some("Base URL for API requests".to_string()),
            }],
            derives: vec!["Debug".to_string(), "Clone".to_string()],
            generics: Vec::new(),
            documentation: Some("Generated API client".to_string()),
            visibility: Visibility::Public,
        };

        RustNode::Struct(struct_def)
    }
}
