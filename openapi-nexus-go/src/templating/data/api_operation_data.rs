//! API operation data for template rendering

use serde::{Deserialize, Serialize};

use crate::ast::GoStruct;
use crate::templating::data::CommonFileHeaderData;

/// Go-specific API method data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoApiMethodData {
    pub name: String, // PascalCase method name
    pub http_method: String,
    pub path: String,
    pub operation_id: String, // Use method name as default
    pub path_params: Vec<GoParameterInfo>,
    pub query_params: Vec<GoParameterInfo>,
    pub header_params: Vec<GoParameterInfo>,
    pub body_param: Option<GoParameterInfo>,
    pub has_request_body: bool,
    pub request_body_content_type: String,
    pub response_type: Option<String>,
    pub description: Option<String>,
}

/// Go-specific parameter info with pre-converted names
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoParameterInfo {
    pub original_name: String,
    pub param_name: String,       // PascalCase for Go
    pub param_name_camel: String, // camelCase for Go
    pub go_type: String,          // Go type as string
    pub required: bool,
    pub description: Option<String>,
}

/// API operation data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiOperationData {
    pub client_struct: GoStruct,
    pub methods: Vec<GoApiMethodData>,
    pub imports: Vec<String>,
    pub tag: String,
    pub tag_pascal_case: String,
    pub tag_snake_case: String,
    pub package_name: String,
    pub sdk_name: String, // Root SDK name (from OpenAPI title)
    pub common_file_header: CommonFileHeaderData,
}

impl ApiOperationData {
    pub fn new(
        client_struct: GoStruct,
        tag: String,
        sdk_name: String,
        common_file_header: CommonFileHeaderData,
    ) -> Self {
        use heck::{ToPascalCase as _, ToSnakeCase as _};
        Self {
            client_struct,
            methods: Vec::new(),
            imports: Vec::new(),
            tag: tag.clone(),
            tag_pascal_case: tag.to_pascal_case(),
            tag_snake_case: tag.to_snake_case(),
            package_name: "operations".to_string(),
            sdk_name,
            common_file_header,
        }
    }

    pub fn with_methods(mut self, methods: Vec<GoApiMethodData>) -> Self {
        self.methods = methods;
        self
    }

    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }
}
