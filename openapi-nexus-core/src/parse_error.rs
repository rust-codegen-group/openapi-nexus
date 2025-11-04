//! Parse errors for OpenAPI files

use snafu::Snafu;

/// Parse errors for OpenAPI files
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ParseError {
    #[snafu(display("Failed to read file '{}': {}", path, source))]
    FileRead {
        path: String,
        source: std::io::Error,
    },

    #[snafu(display("Failed to parse JSON: {}", source))]
    JsonParse { source: serde_json::Error },

    #[snafu(display("Failed to parse YAML: {}", source))]
    YamlParse { source: serde_norway::Error },

    #[snafu(display("Unsupported file format: {}", format))]
    UnsupportedFormat { format: String },
}
