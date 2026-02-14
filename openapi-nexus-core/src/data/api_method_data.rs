//! API method data with raw OpenAPI types

use serde::{Deserialize, Serialize};

use super::parameter_info::ParameterInfo;
use crate::serde::http_method;
use openapi_nexus_spec::oas31::spec::{ObjectOrReference, ObjectSchema, RequestBody};

/// API method data with raw OpenAPI types
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiMethodData {
    pub method_name: String,
    #[serde(with = "http_method")]
    pub http_method: http::Method,
    pub path: String,
    pub path_params: Vec<ParameterInfo>,
    pub query_params: Vec<ParameterInfo>,
    pub header_params: Vec<ParameterInfo>,
    pub request_body: Option<ObjectOrReference<RequestBody>>,
    pub return_type: Option<ObjectOrReference<ObjectSchema>>,
    pub has_auth: bool,
    pub has_error_handling: bool,
}
