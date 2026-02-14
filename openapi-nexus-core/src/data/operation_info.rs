//! Operation information for grouping by tag

use std::collections::BTreeMap;

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::data::api_method_data::ApiMethodData;
use crate::data::parameter_info::ParameterInfo;
use crate::data::{HttpResponse, StatusCode};
use crate::serde::http_method;
use crate::traits::OperationInfoExt;
use openapi_nexus_spec::oas31::spec::{
    Components, ObjectOrReference, ObjectSchema, Operation, Parameter, ParameterIn,
};

/// Operation information for grouping by tag
#[derive(Clone, Serialize, Deserialize)]
pub struct OperationInfo {
    pub path: String,
    #[serde(with = "http_method")]
    pub method: http::Method,
    pub operation: Operation,
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

    fn parameters(&self) -> Vec<Parameter> {
        self.operation
            .parameters
            .iter()
            .filter_map(|param_ref| {
                match param_ref {
                    ObjectOrReference::Object(param) => Some(param.clone()),
                    ObjectOrReference::Ref { .. } => None, // TODO: Resolve references
                }
            })
            .collect()
    }
}

impl OperationInfo {
    /// Convert to ApiMethodData with optional Components for schema resolution
    pub fn to_api_method_data(&self, _components: Option<&Components>) -> ApiMethodData {
        let method_name = self.method_name();

        // Extract parameters
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();

        for param_ref in &self.operation.parameters {
            let param = match param_ref {
                ObjectOrReference::Object(param) => param,
                ObjectOrReference::Ref { .. } => {
                    // TODO: Resolve parameter references
                    continue;
                }
            };

            // Extract schema from parameter
            let schema = param.schema.clone();

            let required = param.required.unwrap_or(false);
            let deprecated = param.deprecated.unwrap_or(false);

            // Extract default value from schema
            let default_value = None; // TODO: Extract from schema

            let param_info = ParameterInfo {
                original_name: param.name.clone(),
                param_name: param.name.clone(),
                schema,
                required,
                deprecated,
                description: param.description.clone(),
                default_value,
                location: param.location.into(),
            };
            match param.location {
                ParameterIn::Path => path_params.push(param_info),
                ParameterIn::Query => query_params.push(param_info),
                ParameterIn::Header => header_params.push(param_info),
                ParameterIn::Cookie => header_params.push(param_info), // Treat cookie as header
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
            has_auth: !self.operation.security.is_empty(),
            has_error_handling: true,
        }
    }

    pub fn collect_responses(
        &self,
        _components: Option<&Components>,
    ) -> (
        BTreeMap<StatusCode, HttpResponse>,
        BTreeMap<StatusCode, HttpResponse>,
        Option<HttpResponse>,
    ) {
        let mut success = BTreeMap::new();
        let mut error = BTreeMap::new();
        let mut default_response = None;

        if let Some(responses) = &self.operation.responses {
            for (status_code, response_ref) in responses {
                let status = StatusCode::new(status_code);
                let response = match response_ref {
                    ObjectOrReference::Object(response) => {
                        HttpResponse::from_openapi(status.clone(), response)
                    }
                    ObjectOrReference::Ref { ref_path, .. } => {
                        // TODO: Resolve response references
                        error!(%status, %ref_path, "Failed to resolve response reference.");
                        continue;
                    }
                };

                if status.is_default() {
                    default_response = Some(response);
                } else if response.is_success() {
                    success.insert(status, response);
                } else {
                    error.insert(status, response);
                }
            }
        }

        (success, error, default_response)
    }
}

/// Extract return type from operation responses
fn extract_return_type_from_responses(
    operation: &Operation,
) -> Option<ObjectOrReference<ObjectSchema>> {
    if let Some(responses) = &operation.responses {
        for (status_code, response_ref) in responses {
            if status_code.starts_with('2')
                && let ObjectOrReference::Object(response) = response_ref
                && let Some(json_content) = response.content.get("application/json")
            {
                return json_content.schema.clone();
            }
        }
    }
    None
}
