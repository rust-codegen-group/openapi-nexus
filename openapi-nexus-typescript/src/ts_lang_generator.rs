//! Main TypeScript code generator

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::{fs, path};

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};
use tracing::warn;
use utoipa::openapi;
use utoipa::openapi::OpenApi;

use crate::config::TsConfig;
use crate::core::GeneratorError;
use crate::generator::{
    api_class_generator::ApiClassGenerator, package_files_generator::PackageFilesGenerator,
    schema_context::SchemaContext, schema_generator::SchemaGenerator,
};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_core::NamingConvention;
use openapi_nexus_core::data::{ApiMethodData, ModelData, RuntimeData};
use openapi_nexus_core::generator_registry::LanguageGenerator;
use openapi_nexus_core::traits::code_generator::LanguageCodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileCategory, FileInfo, FileWriter};

/// Main TypeScript code generator
#[derive(Debug, Clone)]
pub struct TsLangGenerator {
    schema_generator: SchemaGenerator,
    api_class_generator: ApiClassGenerator,
    config: TsConfig,
    templating: Templates,
}

impl TsLangGenerator {
    /// Create a new TypeScript generator
    pub fn new(config: TsConfig) -> Self {
        Self {
            schema_generator: SchemaGenerator,
            api_class_generator: ApiClassGenerator::new(),
            config,
            templating: Templates::new(),
        }
    }

    // Helper methods

    /// Extract OpenAPI metadata for file headers
    fn get_openapi_metadata(
        &self,
        openapi: &OpenApi,
    ) -> (Option<String>, Option<String>, Option<String>) {
        (
            Some(openapi.info.title.clone()),
            openapi.info.description.clone(),
            Some(openapi.info.version.clone()),
        )
    }

    /// Generate TypeScript type definitions from model data
    fn generate_model_type_definitions(
        &self,
        models: Vec<ModelData>,
        components: &openapi::Components,
    ) -> HashMap<String, crate::ast::TsTypeDefinition> {
        let mut schemas = HashMap::new();
        let mut visited = HashSet::new();
        let mut context = SchemaContext::new(&components.schemas, &mut visited);

        for model in models {
            match self.schema_generator.schema_to_ts_type_definition(
                &model.name,
                &model.schema,
                &mut context,
            ) {
                Ok(type_def) => {
                    schemas.insert(model.name, type_def);
                }
                Err(e) => {
                    warn!("Failed to convert schema {}: {}", model.name, e);
                }
            }
        }

        schemas
    }

    /// Generate filename based on naming convention
    fn generate_filename(&self, name: &str) -> String {
        let base_name = match self.config.naming_convention {
            NamingConvention::CamelCase => name.to_lower_camel_case(),
            NamingConvention::KebabCase => name.to_kebab_case(),
            NamingConvention::SnakeCase => name.to_snake_case(),
            NamingConvention::PascalCase => name.to_pascal_case(),
        };

        format!("{}.ts", base_name)
    }

    /// Generate files for all schemas with proper directory structure
    fn generate_files(
        &self,
        api_classes: &HashMap<String, FileInfo>,
        schemas: &HashMap<String, crate::ast::TsTypeDefinition>,
        openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, GeneratorError> {
        let mut files = Vec::new();

        // Generate models files
        let (title, description, version) = self.get_openapi_metadata(openapi);
        for (name, type_def) in schemas {
            let filename = self.generate_filename(name);

            // Emit model content using template
            let content = self
                .templating
                .emit_model(
                    type_def,
                    title.as_deref(),
                    description.as_deref(),
                    version.as_deref(),
                )
                .map_err(|e| GeneratorError::Generic {
                    message: format!("Failed to emit model {}: {}", name, e),
                })?;

            files.push(FileInfo::model(filename, content));
        }

        // Add API class files (already rendered)
        files.extend(api_classes.values().cloned());

        // Generate subdirectory index files
        files.push(self.generate_apis_index_file(api_classes)?);
        files.push(self.generate_models_index_file(schemas)?);

        // Generate main index.ts
        files.push(self.generate_main_index_file());

        // Generate package files if configured
        if self.config.generate_package {
            let package_files = self.generate_package_files(openapi)?;
            files.extend(package_files);
        }

        Ok(files)
    }

    /// Generate apis/index.ts file
    fn generate_apis_index_file(
        &self,
        api_classes: &HashMap<String, FileInfo>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        let mut sorted_api_vec: Vec<(&String, &FileInfo)> = api_classes.iter().collect();
        sorted_api_vec.sort_by(|a, b| a.0.cmp(b.0));
        for (_, file_info) in sorted_api_vec {
            let import_name = file_info.filename.trim_end_matches(".ts");
            exports.push(format!("export * from './{}';", import_name));
        }

        Ok(FileInfo::new(
            "apis/index.ts".to_string(),
            exports.join("\n"),
            FileCategory::ProjectFiles,
        ))
    }

    /// Generate models/index.ts file
    fn generate_models_index_file(
        &self,
        schemas: &HashMap<String, crate::ast::TsTypeDefinition>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        let mut sorted_names: Vec<&String> = schemas.keys().collect();
        sorted_names.sort();
        for name in sorted_names {
            let filename = self.generate_filename(name);
            let import_name = filename.trim_end_matches(".ts");
            exports.push(format!("export * from './{}';", import_name));
        }

        Ok(FileInfo::new(
            "models/index.ts".to_string(),
            exports.join("\n"),
            FileCategory::ProjectFiles,
        ))
    }

    /// Generate main index.ts file
    fn generate_main_index_file(&self) -> FileInfo {
        let exports = [
            // Export runtime files from runtime directory
            "export * from './runtime/api';".to_string(),
            "export * from './runtime/config';".to_string(),
            "export * from './runtime/core';".to_string(),
            // Export all from apis and models
            "export * from './apis';".to_string(),
            "export * from './models';".to_string(),
        ];

        FileInfo::new(
            "index.ts".to_string(),
            exports.join("\n"),
            FileCategory::ProjectFiles,
        )
    }

    /// Generate package files (package.json, tsconfig.json, etc.)
    fn generate_package_files(
        &self,
        openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        if !self.config.generate_package {
            return Ok(Vec::new());
        }

        let package_generator = PackageFilesGenerator::new(&self.config);

        let mut files = vec![
            package_generator.generate_package_json(openapi),
            package_generator.generate_tsconfig(openapi),
        ];
        if self.config.generate_esm_config {
            files.push(package_generator.generate_tsconfig_esm(openapi));
        }

        Ok(files)
    }
}

impl LanguageGenerator for TsLangGenerator {}

impl LanguageCodeGenerator for TsLangGenerator {
    fn language(&self) -> String {
        "typescript".to_string()
    }

    fn framework(&self) -> String {
        "fetch".to_string()
    }

    fn generate_apis(
        &self,
        openapi: &OpenApi,
        _apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let operations_by_tag = self.collect_operations_by_tag(openapi);
        let (title, description, version) = self.get_openapi_metadata(openapi);

        let mut api_classes_map = HashMap::new();
        for (tag, operations) in operations_by_tag {
            if !operations.is_empty() {
                let file_info = self
                    .api_class_generator
                    .generate_api_class(
                        &tag,
                        &operations,
                        &self.templating,
                        title.as_deref(),
                        description.as_deref(),
                        version.as_deref(),
                    )
                    .map_err(|e| GeneratorError::Generic {
                        message: format!("Failed to generate API class for tag {}: {}", tag, e),
                    })?;
                api_classes_map.insert(tag, file_info);
            }
        }

        let generated_files = self
            .generate_files(&api_classes_map, &HashMap::new(), openapi)
            .map_err(|e| GeneratorError::Generic {
                message: format!("File generation error: {}", e),
            })?;

        // Filter to only get API files (index files are in ProjectFiles category)
        let file_infos: Vec<FileInfo> = generated_files
            .into_iter()
            .filter(|file| file.category == FileCategory::Apis)
            .collect();

        Ok(file_infos)
    }

    fn generate_models(
        &self,
        openapi: &OpenApi,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let schemas = if let Some(components) = &openapi.components {
            self.generate_model_type_definitions(models, components)
        } else {
            HashMap::new()
        };

        let generated_files = self
            .generate_files(&HashMap::new(), &schemas, openapi)
            .map_err(|e| GeneratorError::Generic {
                message: format!("File generation error: {}", e),
            })?;

        // Filter to only get model files (index files are in ProjectFiles category)
        let file_infos: Vec<FileInfo> = generated_files
            .into_iter()
            .filter(|file| file.category == FileCategory::Models)
            .collect();

        Ok(file_infos)
    }

    fn generate_runtime(
        &self,
        openapi: &OpenApi,
        runtime_data: RuntimeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let (title, description, version) = self.get_openapi_metadata(openapi);
        let header_obj = minijinja::context! {
            title => title,
            description => description,
            version => version,
        };
        let template_context = minijinja::context! {
            base_path => runtime_data.base_path,
            header => header_obj,
        };
        let file = self
            .templating
            .render_template(TemplateName::Runtime, "runtime.ts", template_context)
            .map_err(|e| GeneratorError::Generic {
                message: format!("Failed to render runtime template: {}", e),
            })?;

        Ok(vec![file])
    }

    fn generate_project_files(
        &self,
        openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        // Generate package files (package.json, tsconfig.json, etc.)
        files.extend(self.generate_package_files(openapi)?);
        // Generate main index.ts (empty HashMaps since we don't need the data for exports)
        files.push(self.generate_main_index_file());

        Ok(files)
    }

    fn generate_readme(
        &self,
        #[allow(unused)] openapi: &OpenApi,
        #[allow(unused)] data: openapi_nexus_core::data::ReadmeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let file = self.templating.render_template(
            TemplateName::Readme,
            "README.md",
            minijinja::Value::from_serialize(data),
        )?;
        Ok(vec![file])
    }
}

impl FileWriter for TsLangGenerator {
    fn write_files(
        &self,
        output_dir: &std::path::Path,
        files: &[FileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Use custom implementation that handles subdirectories properly
        self.write_files_by_category(output_dir, files)
    }

    fn write_files_by_category(
        &self,
        output_dir: &path::Path,
        files: &[FileInfo],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Group files by category
        let mut files_by_category: HashMap<FileCategory, Vec<&FileInfo>> = HashMap::new();
        for file in files {
            files_by_category
                .entry(file.category.clone())
                .or_default()
                .push(file);
        }

        // Write files for each category
        for (category, category_files) in files_by_category {
            let category_dir = match category {
                FileCategory::None => continue,
                FileCategory::Readme => output_dir.to_path_buf(),
                FileCategory::Apis => output_dir.join("apis"),
                FileCategory::Models => output_dir.join("models"),
                FileCategory::ProjectFiles => output_dir.to_path_buf(),
                FileCategory::Runtime => output_dir.join("runtime"),
            };

            // Create directory if it doesn't exist
            if !category_dir.exists() {
                fs::create_dir_all(&category_dir)?;
            }

            // Write files in this category
            for file in category_files {
                let file_path = category_dir.join(&file.filename);

                // Create parent directories if they don't exist (for subdirectories)
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(&file_path, &file.content)?;
            }
        }

        Ok(())
    }
}
