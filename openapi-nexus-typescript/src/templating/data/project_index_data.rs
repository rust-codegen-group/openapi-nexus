//! Project index data for template generation

use serde::{Deserialize, Serialize};

/// Project index data for template context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndexData {
    pub exports: Vec<String>,
}
