//! API method data with raw OpenAPI types

use serde::{Deserialize, Serialize};
use utoipa::openapi::RefOr;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::schema::Schema;

use super::parameter_info::ParameterInfo;
use crate::serde::http_method;

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
    pub request_body: Option<RequestBody>,
    pub return_type: Option<RefOr<Schema>>,
    pub has_auth: bool,
    pub has_error_handling: bool,
}
