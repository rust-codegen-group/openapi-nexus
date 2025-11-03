//! Project index data for template generation

use serde::Serialize;

/// Project index data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ProjectIndexData {
    pub exports: Vec<String>,
}
