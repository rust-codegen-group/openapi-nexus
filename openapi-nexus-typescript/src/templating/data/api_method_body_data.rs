//! API method body data for template generation

use std::sync::Arc;

use minijinja::value::{Object, ObjectRepr, Value};
use serde::{Deserialize, Serialize};

use openapi_nexus_core::data::{
    ApiMethodData as CoreApiMethodData, ParameterInfo as CoreParameterInfo,
};

/// Simplified parameter info for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameterInfo {
    pub name: String,
    pub required: bool,
}

impl From<&CoreParameterInfo> for TemplateParameterInfo {
    fn from(param: &CoreParameterInfo) -> Self {
        Self {
            name: param.name.clone(),
            required: param.required,
        }
    }
}

/// API method body data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMethodBodyData {
    pub path: String,
    #[serde(with = "http_method_str")]
    pub http_method: http::Method,
    pub path_params: Vec<TemplateParameterInfo>,
    pub query_params: Vec<TemplateParameterInfo>,
    pub header_params: Vec<TemplateParameterInfo>,
    pub body_param: Option<TemplateParameterInfo>,
    pub transformer: Option<String>,
}

mod http_method_str {
    use http::Method;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(method: &Method, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(method.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Method, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Method::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl ApiMethodBodyData {
    /// Create from core API method data
    pub fn from_core(core: &CoreApiMethodData, transformer: Option<String>) -> Self {
        let body_param = core.request_body.as_ref().map(|rb| TemplateParameterInfo {
            name: "body".to_string(),
            required: matches!(rb.required, Some(utoipa::openapi::Required::True)),
        });

        Self {
            path: core.path.clone(),
            http_method: core.http_method.clone(),
            path_params: core
                .path_params
                .iter()
                .map(TemplateParameterInfo::from)
                .collect(),
            query_params: core
                .query_params
                .iter()
                .map(TemplateParameterInfo::from)
                .collect(),
            header_params: core
                .header_params
                .iter()
                .map(TemplateParameterInfo::from)
                .collect(),
            body_param,
            transformer,
        }
    }

    /// Convert to JSON for template rendering
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("Failed to serialize ApiMethodBodyData")
    }

    /// Build URL path snippet for TypeScript
    /// Returns code like: `let urlPath = `/users/${userId}/posts/${postId}`;`
    pub fn build_url_path_snippet(&self) -> String {
        // Replace {param} with ${param} for template literals
        let path_expr = self.path.replace('{', "${");
        format!("let urlPath = `{}`;", path_expr)
    }

    /// Build query parameters snippet for TypeScript
    /// Returns code for building queryParameters object
    pub fn build_query_params_snippet(&self) -> String {
        if self.query_params.is_empty() {
            return "const queryParameters: any = {};".to_string();
        }

        let mut code = "const queryParameters: any = {};\n".to_string();
        for param in &self.query_params {
            code.push_str(&format!(
                "if ({} !== undefined) {{\n  queryParameters['{}'] = {};\n}}\n",
                param.name, param.name, param.name
            ));
        }
        code.trim_end().to_string()
    }

    /// Build headers snippet for TypeScript
    /// Returns code for building headerParameters object
    pub fn build_headers_snippet(&self, include_content_type: bool) -> String {
        let mut code = "const headerParameters: Record<string, string> = {\n".to_string();

        if include_content_type && self.body_param.is_some() {
            code.push_str("  'Content-Type': 'application/json',\n");
        }
        code.push_str("  ...this.configuration?.headers,\n");
        code.push_str("};");

        if !self.header_params.is_empty() {
            code.push('\n');
            for param in &self.header_params {
                code.push_str(&format!(
                    "\nif ({} !== undefined) {{\n  headerParameters['{}'] = String({});\n}}",
                    param.name, param.name, param.name
                ));
            }
        }

        code
    }

    /// Build request body snippet
    /// Returns code like: `const body = body;` or `const body = undefined;`
    pub fn build_request_body_snippet(&self) -> String {
        if let Some(ref body_param) = self.body_param {
            format!("const body = {};", body_param.name)
        } else {
            "const body = undefined;".to_string()
        }
    }

    /// Get HTTP method as string
    pub fn http_method_str(&self) -> &str {
        self.http_method.as_str()
    }

    /// Check if has body parameter
    pub fn has_body(&self) -> bool {
        self.body_param.is_some()
    }

    /// Get transformer expression if available
    pub fn transformer_expr(&self) -> Option<&str> {
        self.transformer.as_deref()
    }
}

impl Object for ApiMethodBodyData {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        match key_str {
            // Expose computed snippets as properties
            "url_path_snippet" => Some(Value::from(self.build_url_path_snippet())),
            "query_params_snippet" => Some(Value::from(self.build_query_params_snippet())),
            "headers_snippet" => Some(Value::from(self.build_headers_snippet(self.has_body()))),
            "request_body_snippet" => Some(Value::from(self.build_request_body_snippet())),
            "http_method_str" => Some(Value::from(self.http_method_str())),
            "has_body" => Some(Value::from(self.has_body())),
            // Expose serialized fields
            _ => Value::from_serialize(self).get_attr(key_str).ok(),
        }
    }
}
