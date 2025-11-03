//! API operation data for template generation

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ast::class_definition::TsClassDefinition;
use crate::ast::class_definition::TsImportStatement;
use crate::ast::{TsExpression, TsInterfaceDefinition};
use crate::templating::TemplateName;
use crate::utils::http_method;
use openapi_nexus_core::data::ParameterInfo;

/// HTTP parameter data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpParamData {
    pub http_path: String,
    #[serde(with = "http_method")]
    pub http_method: http::Method,
    pub path_params: Vec<ParameterInfo>,
    pub query_params: Vec<ParameterInfo>,
    pub header_params: Vec<ParameterInfo>,
    pub body_param: Option<ParameterInfo>,
    pub transformer: Option<String>,
}

/// Method template data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodTemplateData {
    pub method_name: String,
    pub body_template: TemplateName,
    pub http_params: Option<HttpParamData>,
    pub convenience_method_name: Option<String>,
    pub convenience_return_type: Option<TsExpression>,
}

/// API operation data for template context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiOperationData {
    pub ts_class: TsClassDefinition,
    pub imports: Vec<TsImportStatement>,
    pub ts_interface: TsInterfaceDefinition,
    pub method_templates: BTreeMap<String, MethodTemplateData>,
}

impl ApiOperationData {
    /// Create new API operation data
    pub fn new(
        ts_class: TsClassDefinition,
        imports: Vec<TsImportStatement>,
        ts_interface: TsInterfaceDefinition,
        method_templates: BTreeMap<String, MethodTemplateData>,
    ) -> Self {
        Self {
            ts_class,
            imports,
            ts_interface,
            method_templates,
        }
    }
}
