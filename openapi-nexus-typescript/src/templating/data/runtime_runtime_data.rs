//! Runtime runtime data for template generation

use serde::{Deserialize, Serialize};

use openapi_nexus_core::data::RuntimeData;

/// Runtime runtime data for template context
#[derive(Debug, Clone, Serialize, Deserialize)]
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
