//! Legacy pipeline orchestration for generators using a 5-phase decomposition.
//!
//! Generators that want the standard decomposition (apis, models, runtime, readme,
//! project_files) implement [`LegacyPipelineCallbacks`] and call [`run_legacy_pipeline`]
//! from their [`CodeGenerator::generate`](crate::traits::CodeGenerator::generate) method.

use std::error::Error;

use super::model_data::ModelData;
use super::operation_info::collect_operations_by_tag;
use super::readme_data::extract_readme_data;
use super::runtime_data::RuntimeData;
use super::{ApiMethodData, ReadmeData};
use crate::traits::file_writer::FileInfo;
use openapi_nexus_spec::OpenApiV31Spec;

/// Callbacks for each generation phase of the legacy pipeline.
pub trait LegacyPipelineCallbacks {
    /// Generate API client files from aggregated API method data.
    fn generate_apis(
        &self,
        openapi: &OpenApiV31Spec,
        apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate model files from component schemas.
    fn generate_models(
        &self,
        openapi: &OpenApiV31Spec,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate runtime utility files.
    fn generate_runtime(
        &self,
        openapi: &OpenApiV31Spec,
        runtime_data: RuntimeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate README file.
    fn generate_readme(
        &self,
        openapi: &OpenApiV31Spec,
        data: ReadmeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate project scaffolding files (go.mod, package.json, etc.).
    fn generate_project_files(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;
}

/// Run the legacy 5-phase generation pipeline.
///
/// This replicates the orchestration that was previously the default
/// `CodeGenerator::generate()` implementation: collect operations by tag,
/// build `ApiMethodData` / `ModelData` / `RuntimeData` / `ReadmeData`,
/// then delegate to the callbacks for each phase.
pub fn run_legacy_pipeline(
    openapi: &OpenApiV31Spec,
    callbacks: &impl LegacyPipelineCallbacks,
) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
    let mut files = Vec::new();

    // Phase 1: APIs
    let all_api_data = collect_operations_by_tag(openapi)
        .into_values()
        .flatten()
        .map(|op_info| op_info.to_api_method_data(openapi.components.as_ref()))
        .collect::<Vec<_>>();
    files.extend(callbacks.generate_apis(openapi, all_api_data)?);

    // Phase 2: Models
    let mut models = Vec::new();
    if let Some(components) = &openapi.components {
        for (name, schema_ref) in &components.schemas {
            models.push(ModelData {
                name: name.clone(),
                schema: schema_ref.clone(),
            });
        }
    }
    files.extend(callbacks.generate_models(openapi, models)?);

    // Phase 3: Runtime
    let runtime_data = RuntimeData::from_openapi(openapi);
    files.extend(callbacks.generate_runtime(openapi, runtime_data)?);

    // Phase 4: Readme
    files.extend(callbacks.generate_readme(openapi, extract_readme_data(openapi))?);

    // Phase 5: Project files
    files.extend(callbacks.generate_project_files(openapi)?);

    Ok(files)
}
