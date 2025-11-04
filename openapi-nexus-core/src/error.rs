//! Error types for the core orchestration

use snafu::Snafu;

use openapi_nexus_parser::ParseError;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    #[snafu(display("Failed to parse OpenAPI specification: {}", source))]
    Parse { source: ParseError },

    #[snafu(display("Failed to generate code: {}", source))]
    Generate {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Unsupported language: {}", language))]
    UnsupportedLanguage { language: String },

    #[snafu(display("Generator not found for language: {}", language))]
    GeneratorNotFound { language: String },
}
