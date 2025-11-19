//! Error types for Go code generation

use snafu::Snafu;

/// Error type for Go code generation
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
    /// Template rendering error
    #[snafu(display("Failed to render template '{}': {}", template_path, source))]
    TemplateRender {
        template_path: String,
        source: minijinja::Error,
    },

    /// Template not found error
    #[snafu(display("Template '{}' not found: {}", template_path, source))]
    TemplateNotFound {
        template_path: String,
        source: minijinja::Error,
    },

    /// API client generation error
    #[snafu(display("Failed to generate API client '{}': {}", client_name, source))]
    ApiClientGeneration {
        client_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Model generation error
    #[snafu(display("Failed to generate model '{}': {}", model_name, source))]
    ModelGeneration {
        model_name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Type mapping error
    #[snafu(display("Failed to map type: {}", source))]
    TypeMapping {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// File I/O error
    #[snafu(display("File I/O error: {}", source))]
    Io { source: std::io::Error },

    /// Config parsing error
    #[snafu(display("Failed to parse config: {}", source))]
    ConfigParse { source: toml::de::Error },

    /// Generic error for cases that don't fit other categories
    #[snafu(display("Generator error: {}", message))]
    Generic { message: String },
}

impl From<std::io::Error> for GeneratorError {
    fn from(err: std::io::Error) -> Self {
        GeneratorError::Io { source: err }
    }
}

impl From<toml::de::Error> for GeneratorError {
    fn from(err: toml::de::Error) -> Self {
        GeneratorError::ConfigParse { source: err }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for GeneratorError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        GeneratorError::Generic {
            message: err.to_string(),
        }
    }
}
