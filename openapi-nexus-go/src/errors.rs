//! Error types for Go code generation.

use snafu::Snafu;

/// Error type for Go code generation.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
    /// IR lowering failed.
    #[snafu(display("IR lowering failed: {}", source))]
    IrLowering {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Sigil-stitch emission failed for a model.
    #[snafu(display("sigil_emit: {}", message))]
    ModelEmission { message: String },

    /// Sigil-stitch emission failed for an API.
    #[snafu(display("sigil_emit_api: {}", message))]
    ApiEmission { message: String },

    /// File I/O error.
    #[snafu(display("File I/O error: {}", source))]
    Io { source: std::io::Error },

    /// Config parsing error.
    #[snafu(display("Failed to parse config: {}", source))]
    ConfigParse { source: toml::de::Error },

    /// Generic error for cases that don't fit other categories.
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
