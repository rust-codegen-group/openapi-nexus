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
    pub http_path: String,
    #[serde(with = "http_method")]
    pub http_method: http::Method,
    pub path_params: Vec<ParameterInfo>,
    pub query_params: Vec<ParameterInfo>,
    pub header_params: Vec<ParameterInfo>,
    pub body_param: Option<ParameterInfo>,
    pub transformer: Option<String>,
    pub uses_request_object: bool,
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
