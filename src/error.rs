//! Errors and reports for the complete OpenAPI Nexus pipeline.

use std::path::PathBuf;

use snafu::Snafu;

use crate::codegen::GeneratorType;
use crate::codegen::traits::WriteError;
use crate::generators::GeneratorError;
use crate::ir::lower::LowerError;
use crate::parser::ParseError;

/// A fatal failure before per-generator execution can begin.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum PipelineError {
    /// The input OpenAPI document could not be parsed.
    #[snafu(display("failed to parse OpenAPI document: {source}"))]
    Parse { source: ParseError },

    /// The parsed document could not be lowered into the shared IR.
    #[snafu(display("failed to lower OpenAPI document: {source}"))]
    Lower { source: LowerError },
}

/// A failure from one selected generator that did not prevent other generators
/// from running.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GenerationFailure {
    /// A generator could not emit its files.
    #[snafu(display("generator {generator} failed: {source}"))]
    Generate {
        generator: GeneratorType,
        source: GeneratorError,
    },

    /// A generator emitted files, but they could not be written.
    #[snafu(display("writing files for generator {generator} to {output_dir:?} failed: {source}"))]
    Write {
        generator: GeneratorType,
        output_dir: PathBuf,
        source: WriteError,
    },
}

/// Results from all selected generators.
#[derive(Debug, Default)]
pub struct GenerationReport {
    /// Generators that emitted and wrote all requested files successfully.
    pub succeeded: Vec<GeneratorType>,
    /// Generator or file-writing failures collected while continuing execution.
    pub failures: Vec<GenerationFailure>,
}

impl GenerationReport {
    /// Returns `true` when every selected generator completed successfully.
    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }
}
