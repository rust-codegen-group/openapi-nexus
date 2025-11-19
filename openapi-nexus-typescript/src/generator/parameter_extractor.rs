//! Parameter extraction utilities for OpenAPI operations

use std::collections::HashMap;

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use tracing::warn;
use utoipa::openapi;

use crate::errors::GeneratorError;
use openapi_nexus_core::data::{OperationInfo, ParameterInfo, ParameterLocation};
use openapi_nexus_core::traits::OpenApiParameterExt as _;

/// Extracted parameters from an OpenAPI operation
#[derive(Debug, Clone)]
pub struct ExtractedParameters {
    /// Path parameters (e.g., {id} in /users/{id})
    pub path_params: Vec<ParameterInfo>,
    /// Query parameters (e.g., ?page=1&limit=10)
    pub query_params: Vec<ParameterInfo>,
    /// Header parameters
    pub header_params: Vec<ParameterInfo>,
    /// Request body parameter
    pub body_param: Option<ParameterInfo>,
}

/// Parameter extractor for OpenAPI operations
#[derive(Debug, Clone)]
pub struct ParameterExtractor;

impl ParameterExtractor {
    /// Extract all parameters from an OpenAPI operation and resolve name conflicts
    pub fn extract_parameters(
        &self,
        op_info: &OperationInfo,
        components: Option<&openapi::Components>,
    ) -> Result<ExtractedParameters, GeneratorError> {
        let mut extracted = self.extract_raw_parameters(op_info, components)?;
        self.resolve_and_apply_name_conflicts(&mut extracted);
        Ok(extracted)
    }

    /// Extract raw parameters from an OpenAPI operation (before conflict resolution)
    fn extract_raw_parameters(
        &self,
        op_info: &OperationInfo,
        components: Option<&openapi::Components>,
    ) -> Result<ExtractedParameters, GeneratorError> {
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();
        let mut body_param = None;

        // Extract path parameters from the path string
        let path_param_names = Self::extract_path_parameter_names(&op_info.path);

        // Extract parameters from the operation
        if let Some(parameters) = &op_info.operation.parameters {
            for param in parameters {
                let original_name = param.name.clone();
                let schema = param.schema.clone();
                let default_value = param.default_value(components);
                let location = match param.parameter_in {
                    openapi::path::ParameterIn::Path => {
                        if path_param_names.contains(&param.name) {
                            // Validate that this parameter actually exists in the path
                            ParameterLocation::Path
                        } else {
                            // If parameter is marked as Path but not in path, treat as query parameter
                            ParameterLocation::Query
                        }
                    }
                    openapi::path::ParameterIn::Query => ParameterLocation::Query,
                    openapi::path::ParameterIn::Header => ParameterLocation::Header,
                    _ => {
                        // Skip other parameter locations for now
                        continue;
                    }
                };

                let param_info = ParameterInfo {
                    original_name: original_name.clone(),
                    param_name: original_name.clone(), // Will be resolved later
                    schema,
                    required: matches!(param.required, openapi::Required::True),
                    deprecated: matches!(param.deprecated, Some(openapi::Deprecated::True)),
                    description: param.description.clone(),
                    default_value,
                    location,
                };

                match location {
                    ParameterLocation::Path => path_params.push(param_info),
                    ParameterLocation::Query => query_params.push(param_info),
                    ParameterLocation::Header => header_params.push(param_info),
                    ParameterLocation::Body => {
                        // Body is handled separately
                        unreachable!("Body parameters should not reach here")
                    }
                }
            }
        }

        // Extract request body parameter
        if let Some(request_body) = &op_info.operation.request_body
            && let Some(json_content) = request_body.content.get("application/json")
            && let Some(schema_ref) = &json_content.schema
        {
            body_param = Some(ParameterInfo {
                original_name: "body".to_string(),
                param_name: "body".to_string(), // Will be resolved later
                schema: Some(schema_ref.clone()),
                required: matches!(request_body.required, Some(openapi::Required::True)),
                deprecated: false, // Request body doesn't have deprecated field in OpenAPI
                description: request_body.description.clone(),
                default_value: None,
                location: ParameterLocation::Body,
            });
        }

        Ok(ExtractedParameters {
            path_params,
            query_params,
            header_params,
            body_param,
        })
    }

    /// Resolve parameter name conflicts and apply resolved names directly to parameters
    fn resolve_and_apply_name_conflicts(&self, extracted: &mut ExtractedParameters) {
        // Collect all parameters with their original names and locations
        let mut all_params: Vec<(&str, ParameterLocation)> = Vec::new();

        for param in &extracted.path_params {
            all_params.push((&param.original_name, param.location));
        }
        for param in &extracted.query_params {
            all_params.push((&param.original_name, param.location));
        }
        for param in &extracted.header_params {
            all_params.push((&param.original_name, param.location));
        }
        if let Some(body_param) = &extracted.body_param {
            all_params.push((&body_param.original_name, body_param.location));
        }

        // Convert all names to camelCase and group by camelCase name
        let mut camel_case_groups: HashMap<String, Vec<(String, ParameterLocation)>> =
            HashMap::new();
        for (original_name, location) in all_params {
            let camel_case = original_name.to_lower_camel_case();
            camel_case_groups
                .entry(camel_case)
                .or_default()
                .push((original_name.to_string(), location));
        }

        // Detect conflicts and build resolution map
        let mut name_map: HashMap<(String, ParameterLocation), String> = HashMap::new();

        for (camel_case_name, params) in &camel_case_groups {
            if params.len() > 1 {
                // Conflict detected - rename ALL conflicting parameters

                warn!(
                    "Parameter name conflict detected for '{}': found {} parameters with the same camelCase name. Renaming all conflicting parameters with location prefixes.",
                    camel_case_name,
                    params.len()
                );

                for (original_name, location) in params {
                    let prefix = match location {
                        ParameterLocation::Body => "body",
                        ParameterLocation::Path => "path",
                        ParameterLocation::Query => "query",
                        ParameterLocation::Header => "header",
                    };
                    let resolved_name = format!("{}{}", prefix, original_name.to_pascal_case())
                        .to_lower_camel_case();

                    name_map.insert((original_name.clone(), *location), resolved_name);
                }
            } else {
                // No conflict - use camelCase name directly
                let (original_name, location) = &params[0];
                let resolved_name = original_name.to_lower_camel_case();
                name_map.insert((original_name.clone(), *location), resolved_name);
            }
        }

        // Apply resolved names to parameters
        for param in &mut extracted.path_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        for param in &mut extracted.query_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        for param in &mut extracted.header_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        // Apply resolved name to body parameter
        if let Some(param) = &mut extracted.body_param
            && let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location))
        {
            param.param_name = resolved.clone();
        }
    }

    /// Convert a serde_json::Value to a TypeScript-compatible string representation
    pub fn value_to_string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(Self::value_to_string).collect();
                format!("[{}]", items.join(", "))
            }
            serde_json::Value::Object(obj) => {
                let pairs: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, Self::value_to_string(v)))
                    .collect();
                format!("{{ {} }}", pairs.join(", "))
            }
        }
    }

    /// Extract path parameter names from a path string
    fn extract_path_parameter_names(path: &str) -> Vec<String> {
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
}
