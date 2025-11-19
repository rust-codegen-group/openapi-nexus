//! Trait for language-specific code generators

use std::collections::HashMap;
use std::error::Error;

use heck::ToKebabCase as _;
use http::Method;
use utoipa::openapi::OpenApi;

use crate::data::{ApiMethodData, ModelData, OperationInfo, ReadmeData, RuntimeData};
use crate::traits::file_writer::FileInfo;
use openapi_nexus_common::{GeneratorType, Language};

/// Trait for code generators
pub trait CodeGenerator {
    /// Returns the language
    fn language(&self) -> Language;

    /// Returns the generator type
    fn generator_type(&self) -> GeneratorType;

    /// Generate files from an OpenAPI specification
    ///
    /// Default implementation calls all category methods and aggregates results.
    fn generate(&self, openapi: &OpenApi) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        // Collect operations by tag and generate API method data
        let all_api_data = self
            .collect_operations_by_tag(openapi)
            .into_values()
            .flatten()
            .map(|op_info| op_info.to_api_method_data(openapi.components.as_ref()))
            .collect::<Vec<_>>();
        files.extend(self.generate_apis(openapi, all_api_data)?);

        // Generate models from schemas
        let mut models = Vec::new();
        if let Some(components) = &openapi.components {
            for (name, schema_ref) in &components.schemas {
                models.push(ModelData {
                    name: name.clone(),
                    schema: schema_ref.clone(),
                });
            }
        }
        files.extend(self.generate_models(openapi, models)?);

        // Generate runtime
        let runtime_data = RuntimeData::from_openapi(openapi);
        files.extend(self.generate_runtime(openapi, runtime_data)?);

        // Generate project files
        files.extend(self.generate_readme(openapi, self.extract_readme_data(openapi))?);
        files.extend(self.generate_project_files(openapi)?);

        Ok(files)
    }

    /// Generate API files from OpenAPI spec and aggregated API method data
    fn generate_apis(
        &self,
        openapi: &OpenApi,
        apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate model files from OpenAPI spec and model data
    fn generate_models(
        &self,
        openapi: &OpenApi,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate runtime files from OpenAPI spec and runtime data
    fn generate_runtime(
        &self,
        openapi: &OpenApi,
        runtime_data: RuntimeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate project files (package.json, README, index files, etc.)
    fn generate_project_files(
        &self,
        openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Generate README file from OpenAPI spec and README data
    fn generate_readme(
        &self,
        openapi: &OpenApi,
        data: ReadmeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>>;

    /// Collect all operations grouped by their tags
    fn collect_operations_by_tag(&self, openapi: &OpenApi) -> HashMap<String, Vec<OperationInfo>> {
        let mut tag_operations = HashMap::new();
        let default_tags = vec!["default".to_string()];

        for (path, path_item) in &openapi.paths.paths {
            let methods = [
                (Method::GET, path_item.get.as_ref()),
                (Method::POST, path_item.post.as_ref()),
                (Method::PUT, path_item.put.as_ref()),
                (Method::DELETE, path_item.delete.as_ref()),
                (Method::PATCH, path_item.patch.as_ref()),
                (Method::OPTIONS, path_item.options.as_ref()),
                (Method::HEAD, path_item.head.as_ref()),
            ];

            for (method, operation_opt) in methods {
                if let Some(operation) = operation_opt {
                    let tags = operation.tags.as_ref().unwrap_or(&default_tags);
                    for tag in tags {
                        tag_operations
                            .entry(tag.clone())
                            .or_insert_with(Vec::new)
                            .push(OperationInfo {
                                path: path.clone(),
                                method: method.clone(),
                                operation: operation.clone(),
                            });
                    }
                }
            }
        }

        tag_operations
    }

    /// Extract README data from OpenAPI specification
    fn extract_readme_data(&self, openapi: &OpenApi) -> ReadmeData {
        let title = openapi.info.title.clone();
        let version = openapi.info.version.clone();
        let description = openapi
            .info
            .description
            .clone()
            .unwrap_or_else(|| "Generated API client".to_string());

        // Generate package name from title
        let package_name = title.to_kebab_case();

        ReadmeData {
            package_name: package_name.clone(),
            title,
            version,
            description,
            example_api_class: "DefaultApi".to_string(),
            generated_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        }
    }
}
