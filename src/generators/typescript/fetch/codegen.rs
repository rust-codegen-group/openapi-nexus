//! TypeScript Fetch code generator

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use super::layout::named_list;
use sigil_stitch::lang::typescript::TypeScript;
use sigil_stitch::prelude::*;

use super::config::TypeScriptFetchConfig;
use super::config::typescript_fetch_config::Toolchain;
use super::plan::{Scope, TypeScriptPlan};
use super::project_files::{render_index_file, render_readme_file, render_runtime_file};
use crate::codegen::NamingConvention;
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::codegen::{GeneratorType, Language};
use crate::generators::request_inputs::{
    RequestInputModel, RequestInputPlan, plan_multipart_request_inputs,
};
use crate::generators::{ArtifactKind, GeneratorError};
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
        request_inputs: &RequestInputPlan,
    ) -> Result<Vec<FileInfo>, GeneratorError> {
        let plan = TypeScriptPlan::new(ir, self.emit_flags());
        self.generate_models_with_plan(plan.ir(), request_inputs, &plan)
    }

    fn emit_flags(&self) -> super::sigil_emit::EmitFlags {
        super::sigil_emit::EmitFlags {
            emit_enum_constants: self.config.emit_enum_constants,
            emit_type_guards: self.config.emit_type_guards,
            property_naming_camel_case: self.config.property_naming
                == super::config::PropertyNaming::CamelCase,
            untagged_union_strategy: self.config.untagged_union_strategy,
        }
    }

    fn generate_models_with_plan(
        &self,
        ir: &IrSpec,
        request_inputs: &RequestInputPlan,
        plan: &TypeScriptPlan<'_>,
    ) -> Result<Vec<FileInfo>, GeneratorError> {
        let ts = TypeScript::new().with_indent(&self.config.indent);
        let flags = plan.flags();
        let mut files = super::sigil_emit::generate_model_files_with_plan(plan, &ts)?;

        if !ir.schemas.is_empty() || !request_inputs.models().is_empty() {
            let mut names: Vec<String> = ir.schemas.keys().cloned().collect();
            names.sort();
            let mut exports: Vec<String> = Vec::new();
            // Track which value names have already been re-exported to avoid
            // collisions between versioned schemas (20260130 / 20260330) that
            // produce identically-named type guard functions.
            let mut used_value_names: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for name in &names {
                let type_name = name.to_pascal_case();
                let filename = self.generate_filename(name);
                let stem = filename.trim_end_matches(".ts");

                let value_names = ir
                    .schemas
                    .get(name)
                    .map(|s| super::sigil_emit::value_exports_for_schema(s, flags, plan, ir))
                    .unwrap_or_default();
                let extra_types = ir
                    .schemas
                    .get(name)
                    .map(|s| super::sigil_emit::extra_type_exports_for_schema(s, flags, plan))
                    .unwrap_or_default();

                if value_names.is_empty() {
                    if extra_types.is_empty() {
                        exports.push(named_list(
                            "export type",
                            std::slice::from_ref(&type_name),
                            &format!("./{stem}"),
                        ));
                    } else {
                        let mut all_types = vec![type_name.clone()];
                        all_types.extend(extra_types);
                        exports.push(named_list("export type", &all_types, &format!("./{stem}")));
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
                        exports.push(named_list("export", &all_names, &format!("./{stem}")));
                    } else {
                        // Type is separate from value exports.
                        // Emit `export type { X, X$Wire }` for types and a separate
                        // `export { xFromJSON, xToJSON }` for value exports.
                        let mut all_types = vec![type_name.clone()];
                        all_types.extend(extra_types);
                        exports.push(named_list("export type", &all_types, &format!("./{stem}")));
                        if !unique_values.is_empty() {
                            exports.push(named_list(
                                "export",
                                &unique_values,
                                &format!("./{stem}"),
                            ));
                        }
                    }
                }
            }
            for model in request_inputs.models() {
                let type_name = model.name.to_pascal_case();
                let filename = self.generate_filename(&model.name);
                let stem = filename.trim_end_matches(".ts");
                exports.push(named_list(
                    "export type",
                    std::slice::from_ref(&type_name),
                    &format!("./{stem}"),
                ));
            }
            files.push(render_index_file(&ir.info, "models/index.ts", &exports));
        }

        for model in request_inputs.models() {
            files.push(self.request_input_model_file(ir, model, plan, &ts)?);
        }

        Ok(files)
    }

    fn request_input_model_file(
        &self,
        ir: &IrSpec,
        model: &RequestInputModel,
        plan: &TypeScriptPlan<'_>,
        ts: &TypeScript,
    ) -> Result<FileInfo, GeneratorError> {
        let header = super::project_files::render_file_header(&ir.info);
        let mut declaration = TypeSpec::builder(&model.name.to_pascal_case(), TypeKind::Interface)
            .visibility(Visibility::Public);
        for field in &model.fields {
            let field_name =
                if self.config.property_naming == super::config::PropertyNaming::CamelCase {
                    field.wire_name.to_lower_camel_case()
                } else {
                    field.wire_name.clone()
                };
            let ty = if field.is_upload() {
                TypeName::importable_type("../runtime/runtime", "UploadFileInput")
            } else {
                plan.value(&field.type_expr, Scope::model(&model.name))
                    .public_type()
            };
            let mut spec = FieldSpec::builder(&ts_property_name(&field_name), ty);
            if !field.required {
                spec = spec.is_optional();
            }
            declaration = declaration.add_field(spec.build().map_err(|source| {
                GeneratorError::emission(ArtifactKind::Model, &model.name, source)
            })?);
        }
        let filename = self.generate_filename(&model.name);
        let file = declaration
            .build()
            .and_then(|declaration| {
                FileSpec::builder_with(&filename, ts.clone())
                    .add_type(declaration)
                    .build()
            })
            .and_then(|file| {
                file.with_lang(super::layout::TypeScript::from(ts.clone()))
                    .render(100)
            })
            .map_err(|source| GeneratorError::emission(ArtifactKind::Model, &model.name, source))?;
        Ok(FileInfo::model(filename, format!("{header}{file}")))
    }

    /// Generate ALL files from the IR spec.
    fn generate_ir(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, GeneratorError> {
        let mut files = Vec::new();
        let plan = TypeScriptPlan::new(ir, self.emit_flags());
        let ir = plan.ir();
        let request_inputs = plan_multipart_request_inputs(ir);

        files.extend(self.generate_apis_with_plan(ir, &request_inputs, &plan)?);
        files.extend(self.generate_models_with_plan(ir, &request_inputs, &plan)?);

        let base_path = ir
            .servers
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_else(|| "http://localhost".to_string());
        files.push(render_runtime_file(
            &ir.info,
            &base_path,
            request_inputs.has_uploads(),
        ));

        files.push(render_readme_file(
            ir,
            self.config.toolchain == Toolchain::Vp
                && self.config.generate_package
                && self.config.include_build_scripts,
        ));

        let has_apis = !ir.operations.is_empty();
        let has_models =
            ir.schemas.values().any(|s| s.is_component) || !request_inputs.models().is_empty();
        files.extend(self.generate_project_files_from_ir(ir, has_apis, has_models));

        Ok(files)
    }

    /// Generate API files from IR operations via the sigil-stitch path.
    ///
    /// The API emitter produces one `{Tag}Api.ts` per tag from the same value plan.
    /// This wrapper adds the `apis/index.ts` aggregator.
    fn generate_apis_with_plan(
        &self,
        ir: &IrSpec,
        request_inputs: &RequestInputPlan,
        plan: &TypeScriptPlan<'_>,
    ) -> Result<Vec<FileInfo>, GeneratorError> {
        let ts = TypeScript::new().with_indent(&self.config.indent);
        let mut files =
            super::sigil_emit_api::generate_api_files_with_plan(ir, request_inputs, plan, &ts)?;

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
            "api-client".to_string(),
            "generated".to_string(),
            "openapi".to_string(),
            "typescript".to_string(),
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

        let (main_file, types_file) = match self.config.toolchain {
            Toolchain::Vp => ("./dist/index.mjs", "./dist/index.d.mts"),
            Toolchain::Tsc => ("./dist/index.js", "./dist/index.d.ts"),
        };

        let mut package_json = serde_json::json!({
            "name": scoped_name,
            "version": version,
            "description": description,
            "type": "module",
            "main": main_file,
            "types": types_file,
            "exports": {
                ".": {
                    "types": types_file,
                    "default": main_file
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
                        "check": "vp check",
                        "fmt": "vp fmt",
                    });
                    package_json["devDependencies"] = serde_json::json!({
                        "typescript": "^7.0.2",
                        "vite-plus": "^1.5.0",
                    });
                }
            }
        }

        let content = render_project_json(&package_json, true);

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

        let content = render_project_json(&tsconfig, false);
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

        let content = render_project_json(&tsconfig_esm, false);
        FileInfo::project("tsconfig.esm.json".to_string(), content)
    }

    /// Generate vite.config.ts for vite-plus toolchain.
    fn generate_vite_config(&self) -> FileInfo {
        let content = r#"import { defineConfig } from 'vite-plus';

export default defineConfig({
  fmt: {
    singleQuote: true,
    ignorePatterns: ['dist/**'],
  },
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

fn ts_property_name(name: &str) -> String {
    if is_js_identifier(name) {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

fn is_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c == '$' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric())
}

impl CodeGenerator for TypeScriptFetchCodeGenerator {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::TypeScriptFetch
    }

    fn generate(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, GeneratorError> {
        self.generate_ir(ir)
    }
}

impl FileWriter for TypeScriptFetchCodeGenerator {}

/// Format `<kind> { N1, N2, ... } from './<file>';` on one line when the
/// single-line form fits within ~100 cols, else break each name onto its own
/// line. Keeps short barrels dense and long ones scannable.
fn format_named_reexport(kind: &str, names: &[String], filename_base: &str) -> String {
    named_list(kind, names, &format!("./{filename_base}"))
}

/// Emit JSON with compact configuration arrays and npm's manifest field order.
fn render_project_json(value: &serde_json::Value, package: bool) -> String {
    fn render(value: &serde_json::Value, depth: usize, package: bool) -> String {
        let pad = "  ".repeat(depth);
        let inner_pad = "  ".repeat(depth + 1);
        match value {
            serde_json::Value::Object(map) if !map.is_empty() => {
                let mut keys = map.keys().collect::<Vec<_>>();
                if package && depth == 0 {
                    let order = [
                        "name",
                        "version",
                        "description",
                        "keywords",
                        "license",
                        "files",
                        "type",
                        "main",
                        "types",
                        "exports",
                        "scripts",
                        "devDependencies",
                    ];
                    keys.sort_by_key(|key| {
                        order
                            .iter()
                            .position(|known| known == &key.as_str())
                            .unwrap_or(order.len())
                    });
                }
                let entries = keys
                    .into_iter()
                    .map(|key| {
                        format!(
                            "{inner_pad}{}: {}",
                            serde_json::to_string(key).expect("JSON key"),
                            render(&map[key], depth + 1, package)
                        )
                    })
                    .collect::<Vec<_>>();
                format!("{{\n{}\n{pad}}}", entries.join(",\n"))
            }
            serde_json::Value::Array(items) if !items.is_empty() => {
                if !package {
                    format!(
                        "[{}]",
                        items
                            .iter()
                            .map(|item| serde_json::to_string(item).expect("JSON value"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else {
                    format!(
                        "[\n{}\n{pad}]",
                        items
                            .iter()
                            .map(|item| format!("{inner_pad}{}", render(item, depth + 1, package)))
                            .collect::<Vec<_>>()
                            .join(",\n")
                    )
                }
            }
            _ => serde_json::to_string(value).expect("JSON value"),
        }
    }
    format!("{}\n", render(value, 0, package))
}
