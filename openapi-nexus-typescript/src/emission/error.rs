//! Error types for TypeScript emission

use snafu::Snafu;

/// Error type for TypeScript emission
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum EmitError {
    #[snafu(display("Template error: {}", message))]
    TemplateError { message: String },
}
