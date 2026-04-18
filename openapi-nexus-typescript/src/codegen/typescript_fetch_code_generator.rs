//! TypeScript Fetch code generator

use std::collections::HashMap;
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use crate::config::TypeScriptFetchConfig;
use crate::errors::GeneratorError;
use crate::templating::data::{CommonFileHeaderData, ProjectIndexData, RuntimeRuntimeData};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::NamingConvention;
use openapi_nexus_core::data::{ReadmeData, RuntimeData};
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus_spec::OpenApiV31Spec;

/// TypeScript Fetch code generator
#[derive(Debug, Clone)]
pub struct TypeScriptFetchCodeGenerator {
    config: TypeScriptFetchConfig,
    templating: Templates,
}

impl TypeScriptFetchCodeGenerator {
    /// Create a new TypeScript Fetch generator
    ///
    /// # Arguments
    /// * `config` - TOML config table
    pub fn new(config: toml::value::Table) -> Self {
        let parsed_config = TypeScriptFetchConfig::from(config);
        Self {
            config: parsed_config,
            templating: Templates::new(GeneratorType::TypeScriptFetch),
        }
    }

    // Helper methods

    /// Generate filename based on naming convention
    fn generate_filename(&self, name: &str) -> String {
        let base_name = match self.config.file_naming_convention {
            NamingConvention::CamelCase => name.to_lower_camel_case(),
            NamingConvention::KebabCase => name.to_kebab_case(),
            NamingConvention::SnakeCase => name.to_snake_case(),
            NamingConvention::PascalCase => name.to_pascal_case(),
        };

        format!("{}.ts", base_name)
    }

    // =========================================================================
    // IR-based generation
    // =========================================================================

    /// Generate all model files from a version-agnostic IR spec.
    ///
    /// Model bodies come from `sigil_emit` (sigil-stitch), not minijinja.
    /// The `models/index.ts` aggregator still uses the minijinja template
    /// so it stays in sync with `apis/index.ts` / top-level `index.ts` until
    /// those migrate too.
    pub fn generate_models_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = crate::sigil_emit::generate_model_files(ir).map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit model generation: {msg}"))
        })?;

        if !ir.schemas.is_empty() {
            let common_file_header = CommonFileHeaderData::new(
                ir.info.title.clone(),
                ir.info.description.clone(),
                ir.info.version.clone(),
            );
            files.push(self.generate_models_index_file(&common_file_header, ir)?);
        }

        Ok(files)
    }

    /// Generate `models/index.ts` from IR schema names.
    fn generate_models_index_file(
        &self,
        common_file_header: &CommonFileHeaderData,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<FileInfo, GeneratorError> {
        let mut type_names: Vec<String> = ir.schemas.keys().cloned().collect();
        type_names.sort();

        let exports: Vec<String> = type_names
            .iter()
            .map(|name| {
                let filename = self.generate_filename(name);
                let import_name = filename.trim_end_matches(".ts");
                format!("export * from './{}';", import_name)
            })
            .collect();

        let project_index = ProjectIndexData { exports };
        let template_context = minijinja::context! {
            common_file_header,
            project_index,
        };

        self.templating
            .render_template(
                TemplateName::ProjectIndex,
                "models/index.ts",
                template_context,
            )
            .map_err(|e| GeneratorError::IndexFileGeneration {
                file_path: "models/index.ts".to_string(),
                source: Box::new(e),
            })
    }

    /// Generate ALL files from the IR spec.
    ///
    /// This is the main entry point for the IR pipeline. It produces:
    /// - Model files from `IrSpec.schemas` (via `IrSchemaGenerator`)
    /// - API files from `IrSpec.operations` (via `ApiOperationGenerator`)
    /// - Runtime, project, readme files from `IrSpec.info`/`IrSpec.servers`
    pub fn generate_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        let common_file_header = CommonFileHeaderData::new(
            ir.info.title.clone(),
            ir.info.description.clone(),
            ir.info.version.clone(),
        );

        // --- APIs (IR path) ---
        files.extend(self.generate_apis_from_ir(ir, &common_file_header)?);

        // --- Models (IR path) ---
        files.extend(self.generate_models_from_ir(ir)?);

        // --- Runtime (IR path) ---
        let base_path = ir
            .servers
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_else(|| "http://localhost".to_string());
        let runtime_data = RuntimeData { base_path };
        let runtime_runtime = RuntimeRuntimeData::from(runtime_data);
        let template_context = minijinja::context! {
            common_file_header,
            runtime_runtime,
        };
        let runtime_file = self
            .templating
            .render_template(TemplateName::Runtime, "runtime.ts", template_context)
            .map_err(|e| GeneratorError::RuntimeTemplate {
                source: Box::new(e),
            })?;
        files.push(runtime_file);

        // --- Readme (IR path) ---
        let readme_data = ReadmeData {
            package_name: ir.info.title.to_kebab_case(),
            title: ir.info.title.clone(),
            version: ir.info.version.clone(),
            description: ir
                .info
                .description
                .clone()
                .unwrap_or_else(|| "Generated API client".to_string()),
            example_api_class: "DefaultApi".to_string(),
            generated_date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        };
        let readme_file = self.templating.render_template(
            TemplateName::Readme,
            "README.md",
            minijinja::Value::from_serialize(readme_data),
        )?;
        files.push(readme_file);

        // --- Project files (IR path) ---
        let has_apis = !ir.operations.is_empty();
        let has_models = ir.schemas.values().any(|s| s.is_component);
        files.extend(self.generate_project_files_from_ir(ir, has_apis, has_models)?);

        Ok(files)
    }

    /// Generate API files from IR operations via the sigil-stitch path.
    ///
    /// `sigil_emit_api::generate_api_files` produces one `{Tag}Api.ts` per tag
    /// (grouping + naming handled inside sigil). This wrapper adds the
    /// `apis/index.ts` aggregator, still rendered via minijinja so it stays in
    /// sync with `models/index.ts` until that path migrates too.
    fn generate_apis_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
        common_file_header: &CommonFileHeaderData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = crate::sigil_emit_api::generate_api_files(ir).map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit_api generation: {msg}"))
        })?;

        if !files.is_empty() {
            let mut api_classes_map: HashMap<String, FileInfo> = HashMap::new();
            for f in &files {
                let tag = f.filename.trim_end_matches("Api.ts").to_string();
                api_classes_map.insert(tag, f.clone());
            }
            files.push(self.generate_apis_index_file_from_ir(common_file_header, &api_classes_map)?);
        }

        Ok(files)
    }

    /// Generate apis/index.ts from IR (no raw spec dependency).
    fn generate_apis_index_file_from_ir(
        &self,
        common_file_header: &CommonFileHeaderData,
        api_classes: &HashMap<String, FileInfo>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        let mut sorted_api_vec: Vec<(&String, &FileInfo)> = api_classes.iter().collect();
        sorted_api_vec.sort_by(|a, b| a.0.cmp(b.0));
        for (_, file_info) in sorted_api_vec {
            let import_name = file_info.filename.trim_end_matches(".ts");
            exports.push(format!("export * from './{}';", import_name));
        }

        let project_index = ProjectIndexData { exports };
        let template_context = minijinja::context! {
            common_file_header,
            project_index,
        };

        self.templating
            .render_template(
                TemplateName::ProjectIndex,
                "apis/index.ts",
                template_context,
            )
            .map_err(|e| GeneratorError::IndexFileGeneration {
                file_path: "apis/index.ts".to_string(),
                source: Box::new(e),
            })
    }

    /// Generate project files (package.json, tsconfig, main index) from IR.
    fn generate_project_files_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
        has_apis: bool,
        has_models: bool,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        // Package files (package.json, tsconfig)
        if self.config.generate_package {
            files.push(self.generate_package_json_from_ir(ir));
            files.push(self.generate_tsconfig_from_config());
            if self.config.generate_esm_config {
                files.push(self.generate_tsconfig_esm_from_config());
            }
        }

        // Main index.ts
        files.push(self.generate_main_index_from_ir(ir, has_apis, has_models)?);

        Ok(files)
    }

    /// Generate package.json from IR info.
    fn generate_package_json_from_ir(&self, ir: &openapi_nexus_ir::types::IrSpec) -> FileInfo {
        let title = ir.info.title.clone();
        let version = ir.info.version.clone();
        let description = ir
            .info
            .description
            .clone()
            .unwrap_or_else(|| format!("TypeScript client for {}", title));

        let license: Option<String> = ir.info.license.as_ref().map(|l| l.name.clone());

        let keywords = vec![
            "openapi".to_string(),
            "api-client".to_string(),
            "typescript".to_string(),
            "generated".to_string(),
        ];

        let package_name = self
            .config
            .package_name
            .clone()
            .unwrap_or_else(|| title.to_kebab_case());
        let scoped_name = if let Some(scope) = &self.config.package_scope {
            format!("{}/{}", scope, package_name)
        } else {
            package_name
        };

        let mut package_json = serde_json::json!({
            "name": scoped_name,
            "version": version,
            "description": description,
            "type": "module",
            "main": "./dist/index.js",
            "types": "./dist/index.d.ts",
            "exports": {
                ".": {
                    "types": "./dist/index.d.ts",
                    "default": "./dist/index.js"
                }
            },
            "files": ["dist"],
            "keywords": keywords
        });

        if let Some(license_name) = license {
            package_json["license"] = serde_json::Value::String(license_name);
        }

        if self.config.include_build_scripts {
            package_json["scripts"] = serde_json::json!({
                "build": "tsc",
            });
        }

        let content =
            serde_json::to_string_pretty(&package_json).unwrap_or_else(|_| "{}".to_string());

        FileInfo::project("package.json".to_string(), content)
    }

    /// Generate tsconfig.json from config only (no spec dependency).
    fn generate_tsconfig_from_config(&self) -> FileInfo {
        let module_str = self.config.ts_module.to_string();
        let mut tsconfig = serde_json::json!({
            "compilerOptions": {
                "target": self.config.ts_target,
                "module": module_str,
                "declaration": true,
                "declarationMap": true,
                "sourceMap": true,
                "outDir": "./dist",
                "rootDir": "./",
                "moduleResolution": "bundler",
                "esModuleInterop": true,
                "skipLibCheck": true,
                "strict": true,
                "forceConsistentCasingInFileNames": true,
                "resolveJsonModule": true,
                "typeRoots": ["node_modules/@types"]
            },
            "include": ["**/*.ts"],
            "exclude": ["dist", "node_modules"]
        });
        tsconfig["compilerOptions"]["lib"] = serde_json::json!(self.config.ts_lib);

        let content = serde_json::to_string_pretty(&tsconfig).unwrap_or_else(|_| "{}".to_string());
        FileInfo::project("tsconfig.json".to_string(), content)
    }

    /// Generate tsconfig.esm.json from config only.
    fn generate_tsconfig_esm_from_config(&self) -> FileInfo {
        let module_str = self.config.ts_module.to_string();
        let tsconfig_esm = serde_json::json!({
            "extends": "./tsconfig.json",
            "compilerOptions": {
                "module": module_str,
                "outDir": "dist/esm"
            }
        });

        let content =
            serde_json::to_string_pretty(&tsconfig_esm).unwrap_or_else(|_| "{}".to_string());
        FileInfo::project("tsconfig.esm.json".to_string(), content)
    }

    /// Generate main index.ts from IR.
    fn generate_main_index_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
        has_apis: bool,
        has_models: bool,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = vec!["export * from './runtime/runtime';".to_string()];

        if has_apis {
            exports.push("export * from './apis';".to_string());
        }

        if has_models {
            exports.push("export * from './models';".to_string());
        }

        let common_file_header = CommonFileHeaderData::new(
            ir.info.title.clone(),
            ir.info.description.clone(),
            ir.info.version.clone(),
        );
        let project_index = ProjectIndexData { exports };
        let template_context = minijinja::context! {
            common_file_header,
            project_index,
        };

        self.templating
            .render_template(TemplateName::ProjectIndex, "index.ts", template_context)
            .map_err(|e| GeneratorError::IndexFileGeneration {
                file_path: "index.ts".to_string(),
                source: Box::new(e),
            })
    }
}

impl CodeGenerator for TypeScriptFetchCodeGenerator {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::TypeScriptFetch
    }

    fn generate(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let ir = openapi_nexus_ir::lower::v31::lower_v31(openapi)?;
        tracing::info!(
            "TypeScript generator using IR pipeline ({} schemas, {} operations)",
            ir.schemas.len(),
            ir.operations.len()
        );
        self.generate_from_ir(&ir)
    }
}

impl FileWriter for TypeScriptFetchCodeGenerator {}
