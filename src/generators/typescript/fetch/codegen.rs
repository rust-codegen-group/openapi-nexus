//! TypeScript Fetch code generator

use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use super::config::TypeScriptFetchConfig;
use super::config::typescript_fetch_config::Toolchain;
use super::project_files::{render_index_file, render_readme_file, render_runtime_file};
use crate::codegen::NamingConvention;
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::codegen::{GeneratorType, Language};
use crate::ir::types::IrSpec;

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
        ir: &crate::ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let flags = super::sigil_emit::EmitFlags {
            emit_enum_constants: self.config.emit_enum_constants,
            emit_type_guards: self.config.emit_type_guards,
            property_naming_camel_case: self.config.property_naming
                == super::config::PropertyNaming::CamelCase,
        };
        let mut files = super::sigil_emit::generate_model_files(ir, flags).map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit model generation: {msg}"))
        })?;

        if !ir.schemas.is_empty() {
            let mut names: Vec<String> = ir.schemas.keys().cloned().collect();
            names.sort();
            let mut exports: Vec<String> = Vec::new();
            // Track which value names have already been re-exported to avoid
            // collisions between versioned schemas (20260130 / 20260330) that
            // produce identically-named type guard functions.
            let mut used_value_names: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let convertible = super::sigil_emit::build_convertible_set(ir, flags);

            for name in &names {
                let type_name = name.to_pascal_case();
                let filename = self.generate_filename(name);
                let stem = filename.trim_end_matches(".ts");

                let value_names = ir
                    .schemas
                    .get(name)
                    .map(|s| super::sigil_emit::value_exports_for_schema(s, flags, &convertible))
                    .unwrap_or_default();
                let extra_types = ir
                    .schemas
                    .get(name)
                    .map(|s| {
                        super::sigil_emit::extra_type_exports_for_schema(s, flags, &convertible)
                    })
                    .unwrap_or_default();

                if value_names.is_empty() {
                    if extra_types.is_empty() {
                        exports.push(format!("export type {{ {type_name} }} from './{stem}';"));
                    } else {
                        let mut all_types = vec![type_name.clone()];
                        all_types.extend(extra_types);
                        let joined = all_types.join(", ");
                        exports.push(format!("export type {{ {joined} }} from './{stem}';"));
                    }
                } else {
                    let type_name_is_value = value_names.contains(&type_name);
                    used_value_names.insert(type_name.clone());

                    let mut unique_values: Vec<String> = Vec::new();
                    for vn in &value_names {
                        if used_value_names.insert(vn.clone()) {
                            unique_values.push(vn.clone());
                        }
                    }

                    if type_name_is_value {
                        // Enum const: the type name IS the value. Single
                        // `export { X }` covers both the type and value sides.
                        let mut all_names = vec![type_name.clone()];
                        all_names.extend(unique_values);
                        let joined = all_names.join(", ");
                        exports.push(format!("export {{ {joined} }} from './{stem}';"));
                    } else {
                        // Type is separate from value exports.
                        // Emit `export type { X, X$Wire }` for types and a separate
                        // `export { xFromJSON, xToJSON }` for value exports.
                        let mut all_types = vec![type_name.clone()];
                        all_types.extend(extra_types);
                        let type_joined = all_types.join(", ");
                        exports.push(format!("export type {{ {type_joined} }} from './{stem}';"));
                        if !unique_values.is_empty() {
                            let joined = unique_values.join(", ");
                            exports.push(format!("export {{ {joined} }} from './{stem}';"));
                        }
                    }
                }
            }
            files.push(render_index_file(&ir.info, "models/index.ts", &exports));
        }

        Ok(files)
    }

    /// Generate ALL files from the IR spec.
    fn generate_ir(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
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
        ir: &crate::ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = super::sigil_emit_api::generate_api_files(
            ir,
            self.config.property_naming == super::config::PropertyNaming::CamelCase,
        )
        .map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit_api generation: {msg}"))
        })?;

        if !files.is_empty() {
            let mut plans = super::sigil_emit_api::collect_api_file_exports(ir);
            plans.sort_by(|a, b| a.filename_base.cmp(&b.filename_base));
            let mut exports: Vec<String> = Vec::new();
            for plan in plans {
                if !plan.type_names.is_empty() {
                    exports.push(format_named_reexport(
                        "export type",
                        &plan.type_names,
                        &plan.filename_base,
                    ));
                }
                if !plan.value_names.is_empty() {
                    exports.push(format_named_reexport(
                        "export",
                        &plan.value_names,
                        &plan.filename_base,
                    ));
                }
            }
            files.push(render_index_file(&ir.info, "apis/index.ts", &exports));
        }

        Ok(files)
    }

    /// Generate project files (package.json, tsconfig, main index) from IR.
    fn generate_project_files_from_ir(
        &self,
        ir: &crate::ir::types::IrSpec,
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
            if self.config.toolchain == Toolchain::Vp {
                files.push(self.generate_vite_config());
            }
        }

        files.push(self.generate_main_index_from_ir(ir, has_apis, has_models));

        files
    }

    /// Generate package.json from IR info.
    fn generate_package_json_from_ir(&self, ir: &crate::ir::types::IrSpec) -> FileInfo {
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
            match self.config.toolchain {
                Toolchain::Tsc => {
                    package_json["scripts"] = serde_json::json!({
                        "build": "tsc",
                    });
                }
                Toolchain::Vp => {
                    package_json["scripts"] = serde_json::json!({
                        "build": "vp pack",
                        "check": "vp check --no-fmt",
                    });
                    package_json["devDependencies"] = serde_json::json!({
                        "typescript": "latest",
                        "vite-plus": "latest",
                    });
                }
            }
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

    /// Generate vite.config.ts for vite-plus toolchain.
    fn generate_vite_config(&self) -> FileInfo {
        let content = r#"import { defineConfig } from 'vite-plus';

export default defineConfig({
  lint: {
    options: {
      typeAware: true,
      typeCheck: true,
    },
    ignorePatterns: ['node_modules', 'dist'],
  },
  pack: {
    entry: ['index.ts'],
    dts: true,
    format: ['esm'],
  },
});
"#;
        FileInfo::project("vite.config.ts".to_string(), content.to_string())
    }

    /// Generate main index.ts from IR.
    fn generate_main_index_from_ir(
        &self,
        ir: &crate::ir::types::IrSpec,
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

    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        self.generate_ir(ir)
    }
}

impl FileWriter for TypeScriptFetchCodeGenerator {}

/// Format `<kind> { N1, N2, ... } from './<file>';` on one line when the
/// single-line form fits within ~100 cols, else break each name onto its own
/// line. Keeps short barrels dense and long ones scannable.
fn format_named_reexport(kind: &str, names: &[String], filename_base: &str) -> String {
    let one_line = format!(
        "{} {{ {} }} from './{}';",
        kind,
        names.join(", "),
        filename_base
    );
    if names.len() <= 1 || one_line.len() <= 100 {
        return one_line;
    }
    let mut out = format!("{} {{\n", kind);
    for name in names {
        out.push_str(&format!("  {},\n", name));
    }
    out.push_str(&format!("}} from './{}';", filename_base));
    out
}
