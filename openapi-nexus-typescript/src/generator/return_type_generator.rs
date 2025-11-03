//! Return type generation for API operations

use http::Method;
use utoipa::openapi;

use crate::ast::{TsExpression, TsPrimitive};
use crate::core::GeneratorError;
use crate::utils::schema_mapper::SchemaMapper;

/// Generator for API operation return types
#[derive(Debug, Clone)]
pub struct ReturnTypeGenerator {
    schema_mapper: SchemaMapper,
}

impl ReturnTypeGenerator {
    /// Create a new return type generator
    pub fn new() -> Self {
        Self {
            schema_mapper: SchemaMapper::new(),
        }
    }

    /// Generate both raw (wrapped) and convenience (unwrapped) return types
    pub fn generate_return_types(
        &self,
        http_method: &Method,
        operation: &openapi::path::Operation,
    ) -> Result<(TsExpression, TsExpression), GeneratorError> {
        // Analyze response schema once
        let response_type = self.find_response_schema(operation);

        // Generate raw return type (wrapped in ApiResponse)
        let raw_return_type =
            self.generate_raw_return_type_from_schema(http_method, response_type.clone());

        // Generate convenience return type (unwrapped)
        let convenience_return_type =
            self.generate_convenience_return_type_from_schema(http_method, response_type);

        Ok((raw_return_type, convenience_return_type))
    }

    /// Find the response schema from operation responses
    fn find_response_schema(
        &self,
        operation: &openapi::path::Operation,
    ) -> Option<openapi::RefOr<openapi::Schema>> {
        for (status_code, response_ref) in operation.responses.responses.iter() {
            if status_code.starts_with('2') {
                match response_ref {
                    openapi::RefOr::T(response) => {
                        if let Some(json_content) = response.content.get("application/json") {
                            return json_content.schema.clone();
                        }
                    }
                    openapi::RefOr::Ref(_) => {
                        // TODO: Handle response references
                    }
                }
            }
        }
        None
    }

    /// Generate raw return type from schema
    fn generate_raw_return_type_from_schema(
        &self,
        http_method: &Method,
        schema_ref: Option<openapi::RefOr<openapi::Schema>>,
    ) -> TsExpression {
        match schema_ref {
            Some(schema_ref) => {
                let return_type = self.schema_mapper.map_ref_or_schema_to_type(&schema_ref);
                let type_str = return_type.to_string_formatted();
                TsExpression::Reference(format!("Promise<JSONApiResponse<{}>>", type_str))
            }
            None => {
                // Fallbacks: DELETE with no content -> VoidApiResponse; otherwise JSON any
                if *http_method == Method::DELETE {
                    TsExpression::Reference("Promise<VoidApiResponse>".to_string())
                } else {
                    TsExpression::Reference("Promise<JSONApiResponse<any>>".to_string())
                }
            }
        }
    }

    /// Generate convenience return type from schema
    fn generate_convenience_return_type_from_schema(
        &self,
        http_method: &Method,
        schema_ref: Option<openapi::RefOr<openapi::Schema>>,
    ) -> TsExpression {
        match schema_ref {
            Some(schema_ref) => self.schema_mapper.map_ref_or_schema_to_type(&schema_ref),
            None => {
                if *http_method == Method::DELETE {
                    TsExpression::Primitive(TsPrimitive::Void)
                } else {
                    TsExpression::Primitive(TsPrimitive::Any)
                }
            }
        }
    }
}

impl Default for ReturnTypeGenerator {
    fn default() -> Self {
        Self::new()
    }
}
