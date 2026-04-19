//! Error types for TypeScript code generation

use snafu::Snafu;

/// Error type for TypeScript code generation
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
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
