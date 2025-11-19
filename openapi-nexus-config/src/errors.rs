//! Unified error types for OpenAPI Nexus configuration

use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// Unified error type for all configuration-related errors
#[derive(Debug)]
pub enum ConfigError {
    /// Error reading config file from filesystem
    FileRead { path: PathBuf, source: io::Error },
    /// Error parsing config file (TOML syntax error)
    FileParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    /// Error parsing generator config overrides from CLI
    ParseOverrides(String),
    /// Configuration validation error
    Validation(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::FileRead { path, source } => {
                write!(f, "Failed to read config file at {:?}: {}", path, source)
            }
            ConfigError::FileParse { path, source } => {
                write!(f, "Failed to parse config file at {:?}: {}", path, source)
            }
            ConfigError::ParseOverrides(msg) => {
                write!(f, "Failed to parse generator config overrides: {}", msg)
            }
            ConfigError::Validation(msg) => {
                write!(f, "Configuration validation error: {}", msg)
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::FileRead { source, .. } => Some(source),
            ConfigError::FileParse { source, .. } => Some(source),
            ConfigError::ParseOverrides(_) => None,
            ConfigError::Validation(_) => None,
        }
    }
}
