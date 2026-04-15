//! Lowering errors.

use std::fmt;

#[derive(Debug)]
pub enum LowerError {
    /// A `$ref` could not be resolved.
    UnresolvedReference { reference: String },
    /// An external reference was encountered (not supported).
    ExternalReference { reference: String },
    /// An unsupported OpenAPI version was encountered.
    UnsupportedVersion { version: String },
    /// A generic lowering error.
    Other { message: String },
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::UnresolvedReference { reference } => {
                write!(f, "unresolved reference: {reference}")
            }
            LowerError::ExternalReference { reference } => {
                write!(f, "external references not supported: {reference}")
            }
            LowerError::UnsupportedVersion { version } => {
                write!(f, "OpenAPI version {version} lowering not yet implemented")
            }
            LowerError::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LowerError {}
