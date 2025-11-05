//! Operation information for grouping by tag

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use serde::{Deserialize, Serialize};
use utoipa::openapi;

use crate::data::api_method_data::ApiMethodData;
use crate::data::parameter_info::ParameterInfo;
use crate::serde::http_method;
use crate::traits::OpenApiParameterExt as _;
use crate::traits::OperationInfoExt;

/// Operation information for grouping by tag
#[derive(Clone, Serialize, Deserialize)]
pub struct OperationInfo {
    pub path: String,
    #[serde(with = "http_method")]
    pub method: http::Method,
    pub operation: openapi::path::Operation,
}

impl OperationInfoExt for OperationInfo {
    fn method_name(&self) -> String {
        if let Some(operation_id) = self.operation.operation_id.as_ref() {
            operation_id.to_lower_camel_case()
        } else {
            // Generate from path and HTTP method
            let method = self.method.as_str();
            let mut name = method.to_lowercase();
            for part in self.path.split('/') {
                if !part.is_empty() && !part.starts_with('{') {
                    name.push_str(&part.to_pascal_case());
                }
            }
            name.to_lower_camel_case()
        }
    }

    fn parameters(&self) -> Vec<openapi::path::Parameter> {
        if let Some(parameters) = &self.operation.parameters {
            parameters.clone()
        } else {
            Vec::new()
        }
    }
}

impl OperationInfo {
    /// Convert to ApiMethodData with optional Components for schema resolution
    pub fn to_api_method_data(&self, components: Option<&openapi::Components>) -> ApiMethodData {
        let method_name = self.method_name();

        // Extract parameters
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();

        if let Some(params) = &self.operation.parameters {
            for param in params {
                // Extract schema from parameter
                let schema = param.schema.clone();

                let required = param.required();
                let deprecated = param.deprecated();

                // Extract default value from schema
                let default_value = param.default_value(components);

                let param_info = ParameterInfo {
                    original_name: param.name.clone(),
                    param_name: param.name.clone(),
                    schema,
                    required,
                    deprecated,
                    description: param.description.clone(),
                    default_value,
                    location: param.parameter_in.clone().into(),
                };
                match param.parameter_in {
                    openapi::path::ParameterIn::Path => path_params.push(param_info),
                    openapi::path::ParameterIn::Query => query_params.push(param_info),
                    openapi::path::ParameterIn::Header => header_params.push(param_info),
                    openapi::path::ParameterIn::Cookie => header_params.push(param_info), // Treat cookie as header
                }
            }
        }

        // Extract return type from responses
        let return_type = extract_return_type_from_responses(&self.operation);
        ApiMethodData {
            method_name,
            http_method: self.method.clone(),
            path: self.path.clone(),
            path_params,
            query_params,
            header_params,
            request_body: self.operation.request_body.clone(),
            return_type,
            has_auth: self.operation.security.is_some(),
            has_error_handling: true,
        }
    }
}

/// Extract return type from operation responses
fn extract_return_type_from_responses(
    operation: &openapi::path::Operation,
) -> Option<openapi::RefOr<openapi::schema::Schema>> {
    for (status_code, response_ref) in operation.responses.responses.iter() {
        if status_code.starts_with('2')
            && let openapi::RefOr::T(response) = response_ref
            && let Some(json_content) = response.content.get("application/json")
        {
            return json_content.schema.clone();
        }
    }
    None
}
