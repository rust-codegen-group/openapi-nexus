//! API operation data for template generation

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ast::{TsExpression, TsInterfaceDefinition};
use crate::templating::TemplateName;
use crate::templating::data::{ApiClassData, ApiImportStatement};
use crate::utils::http_method;
use openapi_nexus_core::data::ParameterInfo;

/// HTTP parameter data for template rendering
#[derive(Debug, Clone, Serialize)]
pub struct HttpParamData {
    /// The resolved API path (with parameter placeholders) used in templates.
    pub http_path: String,
    #[serde(with = "http_method")]
    pub http_method: http::Method,
    /// All path parameters captured for the operation.
    pub path_params: Vec<ParameterInfo>,
    /// Query parameters, preserving ordering for template iteration.
    pub query_params: Vec<ParameterInfo>,
    /// Header parameters available for the operation.
    pub header_params: Vec<ParameterInfo>,
    /// Request body parameter (if present) with metadata for serialization.
    pub body_param: Option<ParameterInfo>,
    /// Name of the request body model (used for import resolution and ToJSON calls).
    pub body_model_name: Option<String>,
    /// Optional request transformer for pre-processing payloads in templates.
    pub transformer: Option<String>,
    /// Whether this method consumes a single request object parameter.
    pub uses_request_object: bool,
    /// Success responses keyed by normalized status code (e.g. 200, 2XX).
    pub success_responses: BTreeMap<String, ResponseTemplateData>,
    /// Error responses keyed by normalized status code.
    pub error_responses: BTreeMap<String, ResponseTemplateData>,
    /// Explicit OpenAPI default response if provided.
    pub default_response: Option<ResponseTemplateData>,
    /// Synthetic catch-all response used when no default is defined.
    pub fallback_response: Option<ResponseTemplateData>,
}

/// Response metadata exposed to templates
#[derive(Debug, Clone, Serialize)]
pub struct ResponseTemplateData {
    /// Raw status identifier (exact, ranged, or descriptive placeholder).
    pub status_code: String,
    /// Whether the response represents a successful HTTP status.
    pub is_success: bool,
    /// Indicates if the response conveys a payload.
    pub has_body: bool,
    /// Resolved TypeScript type for the response body, if any.
    pub body_type: Option<TsExpression>,
    /// Conditional expression used in generated TypeScript (e.g. `response.status === 200`).
    pub status_condition: Option<String>,
    /// API response wrapper class to instantiate (JSON/Text/Blob/Void).
    pub wrapper_class: String,
    /// Complete TypeScript response expression.
    pub response_type: TsExpression,
    /// Optional transformer applied to JSON responses for post-processing.
    pub transformer: Option<String>,
}

/// Method template data for template rendering
#[derive(Debug, Clone, Serialize)]
pub struct MethodTemplateData {
    pub method_name: String,
    pub body_template: TemplateName,
    pub http_params: Option<HttpParamData>,
    pub convenience_method_name: Option<String>,
    pub convenience_return_type: Option<TsExpression>,
}

/// API operation data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ApiOperationData {
    pub ts_class: ApiClassData,
    pub imports: Vec<ApiImportStatement>,
    pub ts_interface: TsInterfaceDefinition,
    pub method_templates: BTreeMap<String, MethodTemplateData>,
    pub request_interfaces: Vec<TsInterfaceDefinition>,
}
