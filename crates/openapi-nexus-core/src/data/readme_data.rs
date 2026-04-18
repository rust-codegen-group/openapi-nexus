//! README data for template generation

use heck::ToKebabCase as _;
use serde::{Deserialize, Serialize};

use openapi_nexus_spec::OpenApiV31Spec;

/// README data for template generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadmeData {
    pub package_name: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub example_api_class: String,
    pub generated_date: String,
}

/// Extract README data from an OpenAPI specification.
pub fn extract_readme_data(openapi: &OpenApiV31Spec) -> ReadmeData {
    let title = openapi.info.title.clone();
    let version = openapi.info.version.clone();
    let description = openapi
        .info
        .description
        .clone()
        .unwrap_or_else(|| "Generated API client".to_string());

    // Generate package name from title
    let package_name = title.to_kebab_case();

    ReadmeData {
        package_name: package_name.clone(),
        title,
        version,
        description,
        example_api_class: "DefaultApi".to_string(),
        generated_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    }
}
