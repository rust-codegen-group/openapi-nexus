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
    JsonParse {
        source: serde_json::Error,
        context: Vec<String>,
    },

    #[snafu(display("Failed to parse YAML: {}", source))]
    YamlParse {
        source: serde_norway::Error,
        context: Vec<String>,
    },

    #[snafu(display("Unsupported file format: {}", format))]
    UnsupportedFormat { format: String },

    #[snafu(display("Failed to deserialize OpenAPI schema from JSON: {}", source))]
    OpenApiDeserializeJson { source: serde_json::Error },

    #[snafu(display("Failed to deserialize OpenAPI schema from YAML: {}", source))]
    OpenApiDeserializeYaml { source: serde_norway::Error },

    #[snafu(display(
        "Unsupported OpenAPI version: '{}'. Supported versions: 3.0.x, 3.1.x, 3.2.x",
        version
    ))]
    UnsupportedVersion { version: String },

    #[snafu(display("Missing 'openapi' version field in specification"))]
    MissingVersionField,
}
