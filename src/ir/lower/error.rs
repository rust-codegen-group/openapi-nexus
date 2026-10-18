//! Lowering errors.

use snafu::Snafu;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum LowerError {
    /// A `$ref` could not be resolved.
    #[snafu(display("unresolved reference: {reference}"))]
    UnresolvedReference { reference: String },
    /// An external reference was encountered (not supported).
    #[snafu(display("external references not supported: {reference}"))]
    ExternalReference { reference: String },
    /// An unsupported OpenAPI version was encountered.
    #[snafu(display("OpenAPI version {version} lowering not yet implemented"))]
    UnsupportedVersion { version: String },
    /// A generic lowering error.
    #[snafu(display("lowering error: {message}"))]
    Other { message: String },
}
