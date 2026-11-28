//! Errors raised while writing generated files.

use std::io;
use std::path::PathBuf;

use snafu::Snafu;

/// A failure while materializing generated files on disk.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum WriteError {
    /// A logical artifact path is not a canonical relative path.
    #[snafu(display(
        "invalid generated output path {path:?}: expected a relative '/'-separated path"
    ))]
    InvalidOutputPath { path: String },

    /// Multiple artifacts resolve to the same file after category layout.
    #[snafu(display("multiple generated files target {path:?}"))]
    DuplicateOutputPath { path: String },

    /// One output needs to be both a file and an ancestor directory.
    #[snafu(display("generated file {path:?} is also needed as a directory for {child:?}"))]
    OutputPathConflict { path: String, child: String },
    /// The output directory or one of its category directories could not be created.
    #[snafu(display("failed to create output directory {path:?}: {source}"))]
    CreateDirectory { path: PathBuf, source: io::Error },

    /// A generated file could not be written.
    #[snafu(display("failed to write generated file {path:?}: {source}"))]
    WriteFile { path: PathBuf, source: io::Error },
}
