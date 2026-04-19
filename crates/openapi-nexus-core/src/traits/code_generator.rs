//! Trait for language-specific code generators

use std::error::Error;

use crate::traits::file_writer::FileInfo;
use crate::{GeneratorType, Language};
use openapi_nexus_spec::OpenApiV31Spec;

/// Trait for code generators.
///
/// Minimal contract: every generator lowers the parsed spec (via
/// [`openapi_nexus_ir::lower`](https://docs.rs/openapi-nexus-ir) or equivalent)
/// and returns a list of files. How a generator decomposes its pipeline
/// internally is up to the implementation.
pub trait CodeGenerator {
    /// Returns the target language.
    fn language(&self) -> Language;

    /// Returns the generator type identifier.
    fn generator_type(&self) -> GeneratorType;

    /// Generate files from an OpenAPI specification.
    fn generate(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;
}
