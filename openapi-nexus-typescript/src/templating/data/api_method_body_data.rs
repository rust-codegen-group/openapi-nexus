//! API method body data for template generation

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
}
