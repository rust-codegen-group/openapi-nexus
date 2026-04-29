//! Trait for language-specific code generators

use std::error::Error;

use super::file_writer::FileInfo;
use crate::ir::types::IrSpec;
use crate::{GeneratorType, Language};

/// Trait for code generators.
///
/// Generators receive a version-agnostic [`IrSpec`] (lowered by the
/// orchestrator) and return a list of files. How a generator decomposes
/// its pipeline internally is up to the implementation.
pub trait CodeGenerator {
    /// Returns the target language.
    fn language(&self) -> Language;

    /// Returns the generator type identifier.
    fn generator_type(&self) -> GeneratorType;

    /// Generate files from a lowered IR specification.
    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;
}
