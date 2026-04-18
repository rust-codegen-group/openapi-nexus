//! Error types for TypeScript code generation

use snafu::Snafu;

/// Error type for TypeScript code generation
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

    /// Runtime template generation error
    #[snafu(display("Failed to render runtime template: {}", source))]
    RuntimeTemplate {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Index file generation error
    #[snafu(display("Failed to render index file '{}': {}", file_path, source))]
    IndexFileGeneration {
        file_path: String,
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

impl From<minijinja::Error> for GeneratorError {
    fn from(err: minijinja::Error) -> Self {
        GeneratorError::Generic {
            message: err.to_string(),
        }
    }
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
