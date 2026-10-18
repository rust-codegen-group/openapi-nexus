//! Unified error types for OpenAPI Nexus configuration

use std::io;
use std::path::PathBuf;

use snafu::Snafu;

/// Unified error type for all configuration-related errors
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ConfigError {
    /// Error reading config file from filesystem
    #[snafu(display("Failed to read config file at {path:?}: {source}"))]
    FileRead { path: PathBuf, source: io::Error },
    /// Error parsing config file (TOML syntax error)
    #[snafu(display("Failed to parse config file at {path:?}: {source}"))]
    FileParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    /// Error parsing generator config overrides from CLI
    #[snafu(display("Failed to parse generator config overrides: {message}"))]
    ParseOverrides { message: String },
    /// Configuration validation error
    #[snafu(display("Configuration validation error: {message}"))]
    Validation { message: String },
}
