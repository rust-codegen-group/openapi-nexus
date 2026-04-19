//! TypeScript Fetch code generator

use std::collections::HashMap;
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use crate::config::TypeScriptFetchConfig;
use crate::project_files::{render_index_file, render_readme_file, render_runtime_file};
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::NamingConvention;
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus_spec::OpenApiV31Spec;

/// TypeScript Fetch code generator
#[derive(Debug, Clone)]
pub struct TypeScriptFetchCodeGenerator {
    config: TypeScriptFetchConfig,
}

impl TypeScriptFetchCodeGenerator {
    /// Create a new TypeScript Fetch generator
    ///
    /// # Arguments
    /// * `config` - TOML config table
    pub fn new(config: toml::value::Table) -> Self {
        Self {
            config: TypeScriptFetchConfig::from(config),
        }
    }

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

    /// Generate all model files from a version-agnostic IR spec.
    ///
    /// Model bodies come from `sigil_emit` (sigil-stitch). The `models/index.ts`
    /// aggregator is assembled here from the IR schema names.
    pub fn generate_models_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = crate::sigil_emit::generate_model_files(ir).map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit model generation: {msg}"))
        })?;

        if !ir.schemas.is_empty() {
            let mut names: Vec<String> = ir.schemas.keys().cloned().collect();
            names.sort();
            let exports: Vec<String> = names
                .iter()
                .map(|name| {
                    let filename = self.generate_filename(name);
                    format!("export * from './{}';", filename.trim_end_matches(".ts"))
                })
                .collect();
            files.push(render_index_file(&ir.info, "models/index.ts", &exports));
        }

        Ok(files)
    }

    /// Generate ALL files from the IR spec.
    pub fn generate_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        files.extend(self.generate_apis_from_ir(ir)?);
        files.extend(self.generate_models_from_ir(ir)?);

        let base_path = ir
            .servers
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_else(|| "http://localhost".to_string());
        files.push(render_runtime_file(&ir.info, &base_path));

        files.push(render_readme_file(ir));

        let has_apis = !ir.operations.is_empty();
        let has_models = ir.schemas.values().any(|s| s.is_component);
        files.extend(self.generate_project_files_from_ir(ir, has_apis, has_models));

        Ok(files)
    }

    /// Generate API files from IR operations via the sigil-stitch path.
    ///
    /// `sigil_emit_api::generate_api_files` produces one `{Tag}Api.ts` per tag.
    /// This wrapper adds the `apis/index.ts` aggregator.
    fn generate_apis_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
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
            let mut sorted: Vec<(&String, &FileInfo)> = api_classes_map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let exports: Vec<String> = sorted
                .into_iter()
                .map(|(_, file_info)| {
                    format!(
                        "export * from './{}';",
                        file_info.filename.trim_end_matches(".ts")
                    )
                })
                .collect();
            files.push(render_index_file(&ir.info, "apis/index.ts", &exports));
        }

        Ok(files)
    }

    /// Generate project files (package.json, tsconfig, main index) from IR.
    fn generate_project_files_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
        has_apis: bool,
        has_models: bool,
    ) -> Vec<FileInfo> {
        let mut files = Vec::new();

        if self.config.generate_package {
            files.push(self.generate_package_json_from_ir(ir));
            files.push(self.generate_tsconfig_from_config());
            if self.config.generate_esm_config {
                files.push(self.generate_tsconfig_esm_from_config());
            }
        }

        files.push(self.generate_main_index_from_ir(ir, has_apis, has_models));

        files
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
    ) -> FileInfo {
        let mut exports = vec!["export * from './runtime/runtime';".to_string()];
        if has_apis {
            exports.push("export * from './apis';".to_string());
        }
        if has_models {
            exports.push("export * from './models';".to_string());
        }

        render_index_file(&ir.info, "index.ts", &exports)
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
