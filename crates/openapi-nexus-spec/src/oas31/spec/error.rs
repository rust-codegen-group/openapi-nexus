use semver::{Error as SemverError, Version};
use snafu::Snafu;

use super::{reference::ErrorRef, schema::ErrorSchema};

/// Spec errors.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ErrorSpec {
    /// Reference error.
    #[snafu(display("Reference error"))]
    Ref { source: ErrorRef },

    /// Schema error.
    #[snafu(display("Schema error"))]
    Schema { source: ErrorSchema },

    /// Semver error.
    #[snafu(display("Semver error"))]
    Semver { source: SemverError },

    /// Unsupported spec file version.
    #[snafu(display("Unsupported spec file version ({})", version))]
    UnsupportedSpecFileVersion { version: Version },
}

impl From<SemverError> for ErrorSpec {
    fn from(source: SemverError) -> Self {
        ErrorSpec::Semver { source }
    }
}
