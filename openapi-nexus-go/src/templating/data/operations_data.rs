//! Operations data for template rendering

use serde::{Deserialize, Serialize};

use crate::templating::data::CommonFileHeaderData;

/// Response type information for operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub name: String,
    pub operation_name: String,
    pub body_type: Option<String>,
}

/// Operations data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationsData {
    pub responses: Vec<OperationResponse>,
    pub common_file_header: CommonFileHeaderData,
    pub module_path: String,
}

impl OperationsData {
    pub fn new(
        responses: Vec<OperationResponse>,
        common_file_header: CommonFileHeaderData,
        module_path: String,
    ) -> Self {
        Self {
            responses,
            common_file_header,
            module_path,
        }
    }
}
