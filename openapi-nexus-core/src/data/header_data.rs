//! Header data for template generation

use serde::{Deserialize, Serialize};

use openapi_nexus_spec::OpenApiV31Spec;

/// Header data for template generation
#[derive(Clone, Serialize, Deserialize)]
pub struct HeaderData {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
}

impl HeaderData {
    pub fn from_openapi(openapi: &OpenApiV31Spec) -> Self {
        Self {
            title: openapi.info.title.clone(),
            description: openapi.info.description.clone(),
            version: openapi.info.version.clone(),
        }
    }
}
