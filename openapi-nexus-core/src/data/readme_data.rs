//! README data for template generation

use serde::{Deserialize, Serialize};

/// README data for template generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadmeData {
    pub package_name: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub install_path: String,
    pub example_api_class: String,
    pub generated_date: String,
}
