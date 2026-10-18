//! Errors raised while writing generated files.

use std::io;
use std::path::PathBuf;

use snafu::Snafu;

/// A failure while materializing generated files on disk.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum WriteError {
    /// The output directory or one of its category directories could not be created.
    #[snafu(display("failed to create output directory {path:?}: {source}"))]
    CreateDirectory { path: PathBuf, source: io::Error },

    /// A generated file could not be written.
    #[snafu(display("failed to write generated file {path:?}: {source}"))]
    WriteFile { path: PathBuf, source: io::Error },
}
