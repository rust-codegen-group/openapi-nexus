//! Runtime data for template generation

use serde::{Deserialize, Serialize};

use openapi_nexus_spec::OpenApiV31Spec;

/// Runtime data for template generation
#[derive(Clone, Serialize, Deserialize)]
pub struct RuntimeData {
    pub base_path: String,
}

impl RuntimeData {
    pub fn from_openapi(openapi: &OpenApiV31Spec) -> Self {
        let base_path = openapi
            .servers
            .first()
            .map(|server| server.url.clone())
            .unwrap_or_else(|| "http://localhost".to_string());
        Self { base_path }
    }
}
