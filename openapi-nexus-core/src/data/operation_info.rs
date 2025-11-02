//! Operation information for grouping by tag

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use serde::{Deserialize, Serialize};
use utoipa::openapi;

use crate::serde::http_method;
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
