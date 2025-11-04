//! Trait for language-specific code generators

use std::collections::HashMap;
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _};
use http::Method;
use openapi_nexus_common::Language;
use utoipa::openapi;
use utoipa::openapi::OpenApi;

use crate::data::{
    ApiMethodData, ModelData, OperationInfo, ParameterInfo, ReadmeData, RuntimeData,
};
use crate::traits::file_writer::FileInfo;

/// Trait for language-specific code generators
pub trait LanguageCodeGenerator {
    /// Returns the language
    fn language(&self) -> Language;

    /// Returns the framework name (e.g., "fetch", "reqwest", "axios")
    fn framework(&self) -> String;

    /// Generate files from an OpenAPI specification
    ///
    /// Default implementation calls all category methods and aggregates results.
    fn generate(&self, openapi: &OpenApi) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        // Collect operations by tag and generate API method data
        let operations_by_tag = self.collect_operations_by_tag(openapi);
        let mut all_api_data = Vec::new();

        for (_, operations) in operations_by_tag {
            for op_info in operations {
                if let Ok(api_data) = self.extract_api_method_data(
                    op_info.path.as_str(),
                    op_info.method,
                    &op_info.operation,
                ) {
                    all_api_data.push(api_data);
                }
            }
        }

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
            install_path: format!("file:path/to/{}", package_name),
            example_api_class: "DefaultApi".to_string(),
            generated_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        }
    }

    /// Extract API method data from operation details
    fn extract_api_method_data(
        &self,
        path: &str,
        http_method: Method,
        operation: &openapi::path::Operation,
    ) -> Result<ApiMethodData, Box<dyn Error + Send + Sync>> {
        let path = path.to_string();

        // Generate method name from operationId or path + method
        let method_name = if let Some(operation_id) = &operation.operation_id {
            operation_id.to_lower_camel_case()
        } else {
            // Generate from path and HTTP method
            let method_str = http_method.as_str();
            let mut name = method_str.to_lowercase();
            for part in path.split('/') {
                if !part.is_empty() && !part.starts_with('{') {
                    name.push_str(&part.to_pascal_case());
                }
            }
            name.to_lower_camel_case()
        };

        // Extract parameters
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();

        if let Some(params) = &operation.parameters {
            for param in params {
                // Extract schema from parameter
                let schema = param.schema.clone();

                let required = matches!(param.required, openapi::Required::True);
                let deprecated = matches!(param.deprecated, Some(openapi::Deprecated::True));

                let param_info = ParameterInfo {
                    name: param.name.clone(),
                    schema,
                    required,
                    deprecated,
                    location: param.parameter_in.clone(),
                };
                match param.parameter_in {
                    openapi::path::ParameterIn::Path => path_params.push(param_info),
                    openapi::path::ParameterIn::Query => query_params.push(param_info),
                    openapi::path::ParameterIn::Header => header_params.push(param_info),
                    openapi::path::ParameterIn::Cookie => header_params.push(param_info), // Treat cookie as header
                }
            }
        }

        // Extract return type from responses
        let return_type = extract_return_type_from_responses(operation);

        Ok(ApiMethodData {
            method_name,
            http_method,
            path,
            path_params,
            query_params,
            header_params,
            request_body: operation.request_body.clone(),
            return_type,
            has_auth: operation.security.is_some(),
            has_error_handling: true,
        })
    }
}

/// Extract return type from operation responses
fn extract_return_type_from_responses(
    operation: &openapi::path::Operation,
) -> Option<openapi::RefOr<openapi::schema::Schema>> {
    for (status_code, response_ref) in operation.responses.responses.iter() {
        if status_code.starts_with('2')
            && let openapi::RefOr::T(response) = response_ref
            && let Some(json_content) = response.content.get("application/json")
        {
            return json_content.schema.clone();
        }
    }
    None
}
