//! Runtime runtime data for template generation

use serde::Serialize;

use openapi_nexus_core::data::RuntimeData;

/// Runtime runtime data for template context
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeRuntimeData {
    pub base_path: String,
}

impl From<RuntimeData> for RuntimeRuntimeData {
    fn from(runtime_data: RuntimeData) -> Self {
        Self {
            base_path: runtime_data.base_path,
        }
    }
}
