//! Common file header data for template generation

use serde::Serialize;

use openapi_nexus_core::data::HeaderData;

/// Common file header data for template context
#[derive(Debug, Clone, Serialize)]
pub struct CommonFileHeaderData {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
}

impl From<HeaderData> for CommonFileHeaderData {
    fn from(header_data: HeaderData) -> Self {
        Self {
            title: header_data.title,
            description: header_data.description,
            version: header_data.version,
        }
    }
}

impl CommonFileHeaderData {
    /// Create new common file header data
    pub fn new(title: String, description: Option<String>, version: String) -> Self {
        Self {
            title,
            description,
            version,
        }
    }
}
