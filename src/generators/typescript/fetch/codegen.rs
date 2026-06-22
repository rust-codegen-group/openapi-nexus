//! TypeScript Fetch code generator

use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use sigil_stitch::lang::typescript::TypeScript;

use super::config::TypeScriptFetchConfig;
use super::config::typescript_fetch_config::Toolchain;
use super::project_files::{render_index_file, render_readme_file, render_runtime_file};
use crate::codegen::NamingConvention;
use crate::codegen::traits::code_generator::CodeGenerator;
use crate::codegen::traits::file_writer::{FileInfo, FileWriter};
use crate::codegen::{GeneratorType, Language};
use crate::generators::request_inputs::{
    RequestInputField, RequestInputFieldKind, RequestInputModel, RequestInputPlan,
    plan_multipart_request_inputs,
};
use crate::ir::types::{IrPrimitive, IrSpec, IrTypeExpr};

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
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let ts = TypeScript::new().with_indent(&self.config.indent);
        let flags = super::sigil_emit::EmitFlags {
            emit_enum_constants: self.config.emit_enum_constants,
            emit_type_guards: self.config.emit_type_guards,
            property_naming_camel_case: self.config.property_naming
                == super::config::PropertyNaming::CamelCase,
        };
        let mut files = super::sigil_emit::generate_model_files(ir, flags, &ts).map_err(|msg| {
            Box::<dyn Error + Send + Sync>::from(format!("sigil_emit model generation: {msg}"))
        })?;

        if !ir.schemas.is_empty() || !request_inputs.models().is_empty() {
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
                    .map(|s| {
                        super::sigil_emit::value_exports_for_schema(s, flags, &convertible, ir)
                    })
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
            for model in request_inputs.models() {
                let type_name = model.name.to_pascal_case();
                let filename = self.generate_filename(&model.name);
                let stem = filename.trim_end_matches(".ts");
                exports.push(format!("export type {{ {type_name} }} from './{stem}';"));
            }
            files.push(render_index_file(&ir.info, "models/index.ts", &exports));
        }

        for model in request_inputs.models() {
            files.push(self.request_input_model_file(ir, model));
        }

        Ok(files)
    }

    fn request_input_model_file(&self, ir: &IrSpec, model: &RequestInputModel) -> FileInfo {
        let header = super::project_files::render_file_header(&ir.info);
        let mut imports: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut needs_upload = false;
        for field in &model.fields {
            if field.is_upload() {
                needs_upload = true;
            } else {
                collect_ts_model_imports(&field.type_expr, &mut imports);
            }
        }

        let mut content = String::new();
        content.push_str(&header);
        if needs_upload {
            content.push_str("import type { UploadFileInput } from '../runtime/runtime';\n");
        }
        for (schema_name, type_names) in imports {
            let filename = self.generate_filename(&schema_name);
            let stem = filename.trim_end_matches(".ts");
            let names = type_names.into_iter().collect::<Vec<_>>().join(", ");
            content.push_str(&format!("import type {{ {names} }} from './{stem}';\n"));
        }
        if needs_upload || !model.fields.is_empty() {
            content.push('\n');
        }

        content.push_str(&format!(
            "export interface {} {{\n",
            model.name.to_pascal_case()
        ));
        for field in &model.fields {
            let field_name =
                if self.config.property_naming == super::config::PropertyNaming::CamelCase {
                    field.wire_name.to_lower_camel_case()
                } else {
                    field.wire_name.clone()
                };
            let ty = request_input_field_ts_type(field);
            let optional = if field.required { "" } else { "?" };
            content.push_str(&format!(
                "  {}{}: {};\n",
                ts_property_name(&field_name),
                optional,
                ty
            ));
        }
        content.push_str("}\n");

        FileInfo::model(self.generate_filename(&model.name), content)
    }

    /// Generate ALL files from the IR spec.
    fn generate_ir(&self, ir: &IrSpec) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();
        let request_inputs = plan_multipart_request_inputs(ir);

        files.extend(self.generate_apis_from_ir(ir, &request_inputs)?);
        files.extend(self.generate_models_from_ir(ir, &request_inputs)?);

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

        files.push(render_readme_file(ir));

        let has_apis = !ir.operations.is_empty();
        let has_models =
            ir.schemas.values().any(|s| s.is_component) || !request_inputs.models().is_empty();
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
        request_inputs: &RequestInputPlan,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let ts = TypeScript::new().with_indent(&self.config.indent);
        let mut files = super::sigil_emit_api::generate_api_files(
            ir,
            self.config.property_naming == super::config::PropertyNaming::CamelCase,
            request_inputs,
            &ts,
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

fn request_input_field_ts_type(field: &RequestInputField) -> String {
    match field.kind {
        RequestInputFieldKind::UploadFile { .. } => "UploadFileInput".to_string(),
        RequestInputFieldKind::SchemaValue => ts_type_string(&field.type_expr),
    }
}

fn ts_type_string(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Named(name) => name.to_pascal_case(),
        IrTypeExpr::Primitive(p) => match p {
            IrPrimitive::Binary => "Blob | File".to_string(),
            IrPrimitive::String
            | IrPrimitive::Date
            | IrPrimitive::DateTime
            | IrPrimitive::Uuid
            | IrPrimitive::StringWithFormat(_) => "string".to_string(),
            IrPrimitive::Integer
            | IrPrimitive::IntegerWithFormat(_)
            | IrPrimitive::Number
            | IrPrimitive::NumberWithFormat(_) => "number".to_string(),
            IrPrimitive::Boolean => "boolean".to_string(),
        },
        IrTypeExpr::Array(inner) => format!("readonly {}[]", ts_type_string_nested(inner)),
        IrTypeExpr::Nullable(inner) => {
            format!("{} | null", parenthesize_union(&ts_type_string(inner)))
        }
        IrTypeExpr::StringLiteral(value) => {
            format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
        }
        IrTypeExpr::StringEnum(values) => values
            .iter()
            .map(|value| format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'")))
            .collect::<Vec<_>>()
            .join(" | "),
        IrTypeExpr::Map(inner) => format!("Record<string, {}>", ts_type_string(inner)),
        IrTypeExpr::Union(members) => members
            .iter()
            .map(ts_type_string)
            .collect::<Vec<_>>()
            .join(" | "),
        IrTypeExpr::Any => "unknown".to_string(),
    }
}

fn ts_type_string_nested(expr: &IrTypeExpr) -> String {
    match expr {
        IrTypeExpr::Array(inner) => format!("{}[]", ts_type_string_nested(inner)),
        other => parenthesize_union(&ts_type_string(other)),
    }
}

fn parenthesize_union(ty: &str) -> String {
    if ty.contains(" | ") {
        format!("({ty})")
    } else {
        ty.to_string()
    }
}

fn collect_ts_model_imports(
    expr: &IrTypeExpr,
    imports: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    match expr {
        IrTypeExpr::Named(name) => {
            imports
                .entry(name.clone())
                .or_default()
                .insert(name.to_pascal_case());
        }
        IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
            collect_ts_model_imports(inner, imports);
        }
        IrTypeExpr::Union(members) => {
            for member in members {
                collect_ts_model_imports(member, imports);
            }
        }
        _ => {}
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
