//! Common file header data for templates

use serde::{Deserialize, Serialize};

use openapi_nexus_core::data::HeaderData;

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

/// Common file header data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonFileHeaderData {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
}

impl From<HeaderData> for CommonFileHeaderData {
    fn from(header: HeaderData) -> Self {
        Self {
            title: header.title,
            description: header.description,
            version: header.version,
        }
    }
}
