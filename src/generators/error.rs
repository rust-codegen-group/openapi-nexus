//! Errors raised while planning and emitting generated source.

use derive_more::Display;
use sigil_stitch::error::SigilStitchError;
use snafu::Snafu;

/// The kind of generated artifact that failed.
#[derive(Debug, Clone, Copy, Display)]
pub enum ArtifactKind {
    /// An API client source file.
    #[display("API")]
    Api,
    /// A model source file.
    #[display("model")]
    Model,
    /// A generated runtime source file.
    #[display("runtime")]
    Runtime,
    /// A generated project or configuration file.
    #[display("project")]
    Project,
}

/// A failure while planning or emitting one generator's source files.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GeneratorError {
    /// A sigil-stitch structure could not be built or rendered.
    #[snafu(display("{artifact} {name}: {source}"))]
    Emission {
        artifact: ArtifactKind,
        name: String,
        source: Box<SigilStitchError>,
    },

    /// The IR contains a schema this generator cannot represent.
    #[snafu(display("model {schema} has unsupported schema kind {kind}"))]
    UnsupportedSchema { schema: String, kind: String },

    /// A generator-specific plan or input is invalid.
    #[snafu(display("{artifact} {name}: {reason}"))]
    Invalid {
        artifact: ArtifactKind,
        name: String,
        reason: String,
    },
}

impl GeneratorError {
    /// Wrap a sigil-stitch failure with the generated artifact context.
    pub(crate) fn emission(
        artifact: ArtifactKind,
        name: impl Into<String>,
        source: SigilStitchError,
    ) -> Self {
        Self::Emission {
            artifact,
            name: name.into(),
            source: Box::new(source),
        }
    }
}
