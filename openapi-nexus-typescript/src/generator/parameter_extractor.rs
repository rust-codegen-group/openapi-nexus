//! Parameter extraction utilities for OpenAPI operations

use utoipa::openapi;
use utoipa::openapi::path::Operation;

use crate::ast::TsExpression;
use crate::core::GeneratorError;
use crate::utils::schema_mapper::SchemaMapper;
use openapi_nexus_core::data::ParameterInfo;

/// Extracted parameters from an OpenAPI operation
#[derive(Debug, Clone)]
pub struct ExtractedParameters {
    /// Path parameters (e.g., {id} in /users/{id})
    pub path_params: Vec<TsParameterInfo>,
    /// Query parameters (e.g., ?page=1&limit=10)
    pub query_params: Vec<TsParameterInfo>,
    /// Header parameters
    pub header_params: Vec<TsParameterInfo>,
    /// Request body parameter
    pub body_param: Option<TsParameterInfo>,
}

/// Information about a parameter
#[derive(Debug, Clone)]
pub struct TsParameterInfo {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub type_expr: TsExpression,
    /// Whether the parameter is required
    pub required: bool,
    /// Parameter description
    pub description: Option<String>,
    /// Default value if any
    pub default_value: Option<String>,
}

/// Parameter extractor for OpenAPI operations
#[derive(Debug, Clone)]
pub struct ParameterExtractor {
    schema_mapper: SchemaMapper,
}

impl Default for ParameterExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl ParameterExtractor {
    /// Create a new parameter extractor
    pub fn new() -> Self {
        Self {
            schema_mapper: SchemaMapper::new(),
        }
    }

    /// Extract all parameters from an OpenAPI operation
    pub fn extract_parameters(
        &self,
        operation: &Operation,
        path: &str,
    ) -> Result<ExtractedParameters, GeneratorError> {
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();
        let mut body_param = None;

        // Extract path parameters from the path string
        let path_param_names = self.extract_path_parameter_names(path);

        // Extract parameters from the operation
        if let Some(parameters) = &operation.parameters {
            for param in parameters {
                let param_info = TsParameterInfo {
                    name: param.name.clone(),
                    type_expr: if let Some(schema) = &param.schema {
                        self.map_parameter_schema_to_type(schema)
                    } else {
                        TsExpression::Primitive(crate::ast::TsPrimitive::String)
                    },
                    required: matches!(param.required, openapi::Required::True),
                    description: param.description.clone(),
                    default_value: None, // TODO: Extract default value from schema
                };

                match param.parameter_in {
                    openapi::path::ParameterIn::Path => {
                        // Validate that this parameter actually exists in the path
                        if path_param_names.contains(&param.name) {
                            path_params.push(param_info);
                        } else {
                            // If parameter is marked as Path but not in path, treat as query parameter
                            query_params.push(param_info);
                        }
                    }
                    openapi::path::ParameterIn::Query => {
                        query_params.push(param_info);
                    }
                    openapi::path::ParameterIn::Header => {
                        header_params.push(param_info);
                    }
                    _ => {
                        // Skip other parameter locations for now
                    }
                }
            }
        }

        // Extract request body parameter
        if let Some(request_body) = &operation.request_body
            && let Some(json_content) = request_body.content.get("application/json")
            && let Some(schema_ref) = &json_content.schema
        {
            body_param = Some(TsParameterInfo {
                name: "body".to_string(),
                type_expr: self.map_schema_ref_to_type(schema_ref),
                required: matches!(request_body.required, Some(openapi::Required::True)),
                description: request_body.description.clone(),
                default_value: None,
            });
        }

        Ok(ExtractedParameters {
            path_params,
            query_params,
            header_params,
            body_param,
        })
    }

    /// Extract path parameter names from a path string
    fn extract_path_parameter_names(&self, path: &str) -> Vec<String> {
        let mut param_names = Vec::new();
        let mut chars = path.chars();

        while let Some(c) = chars.next() {
            if c == '{' {
                let mut param_name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    param_name.push(c);
                }
                if !param_name.is_empty() {
                    param_names.push(param_name);
                }
            }
        }

        param_names
    }

    /// Map parameter schema to TypeScript type
    fn map_parameter_schema_to_type(
        &self,
        schema_ref: &openapi::RefOr<openapi::Schema>,
    ) -> TsExpression {
        self.schema_mapper.map_ref_or_schema_to_type(schema_ref)
    }

    /// Map schema reference to TypeScript type
    fn map_schema_ref_to_type(&self, schema_ref: &openapi::RefOr<openapi::Schema>) -> TsExpression {
        self.schema_mapper.map_ref_or_schema_to_type(schema_ref)
    }

    /// Extract ParameterInfo for template rendering
    pub fn extract_core_parameters(
        &self,
        operation: &Operation,
        path: &str,
    ) -> Result<ExtractedCoreParameters, GeneratorError> {
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();
        let mut body_param = None;

        let path_param_names = self.extract_path_parameter_names(path);

        // Extract parameters from the operation
        if let Some(parameters) = &operation.parameters {
            for param in parameters {
                let param_info = ParameterInfo {
                    name: param.name.clone(),
                    schema: param.schema.clone(),
                    required: matches!(param.required, openapi::Required::True),
                    deprecated: matches!(param.deprecated, Some(openapi::Deprecated::True)),
                    location: param.parameter_in.clone(),
                };

                match param.parameter_in {
                    openapi::path::ParameterIn::Path => {
                        if path_param_names.contains(&param.name) {
                            path_params.push(param_info);
                        } else {
                            // If parameter is marked as Path but not in path, treat as query parameter
                            query_params.push(param_info);
                        }
                    }
                    openapi::path::ParameterIn::Query => {
                        query_params.push(param_info);
                    }
                    openapi::path::ParameterIn::Header => {
                        header_params.push(param_info);
                    }
                    openapi::path::ParameterIn::Cookie => {
                        header_params.push(param_info);
                    }
                    #[allow(unreachable_patterns)]
                    _ => {
                        // Skip any other parameter locations (should not occur in OpenAPI spec)
                    }
                }
            }
        }

        // Extract request body parameter
        if let Some(request_body) = &operation.request_body {
            body_param = Some(ParameterInfo {
                name: "body".to_string(),
                schema: None, // Could extract from RequestBody.content if needed
                required: matches!(request_body.required, Some(openapi::Required::True)),
                deprecated: false,
                location: openapi::path::ParameterIn::Path, // Placeholder
            });
        }

        Ok(ExtractedCoreParameters {
            path_params,
            query_params,
            header_params,
            body_param,
        })
    }
}

/// Extracted ParameterInfo for template rendering
#[derive(Debug, Clone)]
pub struct ExtractedCoreParameters {
    pub path_params: Vec<ParameterInfo>,
    pub query_params: Vec<ParameterInfo>,
    pub header_params: Vec<ParameterInfo>,
    pub body_param: Option<ParameterInfo>,
}
