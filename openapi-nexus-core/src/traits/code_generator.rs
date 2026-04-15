//! Trait for language-specific code generators

use std::error::Error;

use crate::traits::file_writer::FileInfo;
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_spec::OpenApiV31Spec;

/// Trait for code generators.
///
/// This is the minimal contract that all generators must fulfill.
/// Generators that want a 5-phase decomposition (apis, models, runtime, readme,
/// project files) can implement [`LegacyPipelineCallbacks`](crate::data::LegacyPipelineCallbacks)
/// and delegate to [`run_legacy_pipeline`](crate::data::run_legacy_pipeline).
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
