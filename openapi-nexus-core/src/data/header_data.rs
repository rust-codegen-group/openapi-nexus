//! Header data for template generation

use openapi_nexus_ir::OpenApi;
use serde::{Deserialize, Serialize};

/// Header data for template generation
#[derive(Clone, Serialize, Deserialize)]
pub struct HeaderData {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
}

impl HeaderData {
    pub fn from_openapi(openapi: &OpenApi) -> Self {
        Self {
            title: openapi.info.title.clone(),
            description: openapi.info.description.clone(),
            version: openapi.info.version.clone(),
        }
    }
}
