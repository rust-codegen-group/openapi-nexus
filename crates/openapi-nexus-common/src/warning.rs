//! Parse warning for non-fatal issues

use crate::location::SourceLocation;

/// Parse warning for non-fatal issues
#[derive(Debug, Clone)]
pub struct ParseWarning {
    pub message: String,
    pub location: SourceLocation,
}

impl ParseWarning {
    pub fn new(message: String, location: SourceLocation) -> Self {
        Self { message, location }
    }
}
