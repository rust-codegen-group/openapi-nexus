//! TypeScript Fetch code generator

use std::collections::{HashMap, HashSet};
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};

use crate::ast::{TsTypeAliasDefinition, TsTypeDefinition};
use crate::config::TypeScriptFetchConfig;
use crate::errors::GeneratorError;
use crate::generator::{
    api_operation_generator::ApiOperationGenerator, ir_schema_generator::IrSchemaGenerator,
    package_files_generator::PackageFilesGenerator, schema_context::SchemaContext,
    schema_generator::SchemaGenerator,
};
use crate::templating::data::{
    ApiImportSpecifier, ApiImportStatement, ApiImportStatements, CommonFileHeaderData,
    ModelEnumData, ModelInterfaceData, ModelTypeAliasData, ProjectIndexData, RuntimeRuntimeData,
};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::NamingConvention;
use openapi_nexus_core::data::{ApiMethodData, HeaderData, ModelData, ReadmeData, RuntimeData};
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus_spec::OpenApiV31Spec;

/// TypeScript Fetch code generator
#[derive(Debug, Clone)]
pub struct TypeScriptFetchCodeGenerator {
    schema_generator: SchemaGenerator,
    api_operation_generator: ApiOperationGenerator,
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
            schema_generator: SchemaGenerator,
            api_operation_generator: ApiOperationGenerator::new(),
            config: parsed_config,
            templating: Templates::new(GeneratorType::TypeScriptFetch),
        }
    }

    // Helper methods

    /// Generate TypeScript type definitions from model data
    fn generate_model_type_definitions(
        &self,
        models: Vec<ModelData>,
        components: &openapi_nexus_spec::oas31::spec::Components,
    ) -> (
        HashMap<String, TsTypeDefinition>,
        HashMap<String, (String, String)>,
    ) {
        let mut schemas = HashMap::new();
        let mut visited = HashSet::new();
        let mut inline_interfaces = HashMap::new();
        let mut enum_discriminators = HashMap::new();
        let mut union_discriminators = HashMap::new();
        let mut context = SchemaContext::new(
            &components.schemas,
            &mut visited,
            &mut inline_interfaces,
            &mut enum_discriminators,
            &mut union_discriminators,
        );

        for model in models {
            let type_def = self.schema_generator.schema_to_ts_type_definition(
                &model.name,
                &model.schema,
                &mut context,
            );
            schemas.insert(model.name, type_def);
        }

        // Collect all generated inline interfaces and add them to schemas.
        // Use the original_name from the type definition as the key for consistency.
        // Do not overwrite an existing component schema (component schema wins).
        for type_def in context.get_inline_interfaces().values() {
            let original_name = type_def.original_name().to_string();
            schemas
                .entry(original_name)
                .or_insert_with(|| type_def.clone());
        }

        (schemas, enum_discriminators)
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

    /// Create ModelTypeAliasData with imports and instanceOf function imports for union members
    fn create_model_type_alias_data(
        &self,
        type_alias: &TsTypeAliasDefinition,
        imports: ApiImportStatements,
        schemas: &HashMap<String, TsTypeDefinition>,
    ) -> Result<ModelTypeAliasData, GeneratorError> {
        let mut model_type_alias =
            ModelTypeAliasData::new(type_alias.clone()).with_imports(imports);

        // Add imports for instanceOf and FromJSONTyped functions for union members that are interfaces
        let union_members: Vec<_> = model_type_alias
            .union_members()
            .map(|members| members.to_vec())
            .unwrap_or_default();

        for member in &union_members {
            // Skip Kind types - they're string unions, not interfaces, so instanceOf doesn't make sense
            // This also avoids importing from Kind files which don't export instanceOf functions
            if member.is_interface && !member.ts_name.ends_with("Kind") {
                // Find the file for this member type
                let member_type_def = schemas
                    .values()
                    .find(|type_def| type_def.ts_name() == member.ts_name);

                if let Some(member_type_def) = member_type_def {
                    let member_filename = self.generate_filename(member_type_def.ts_name());
                    let import_path = format!("./{}", member_filename.trim_end_matches(".ts"));

                    // Find or create the import statement for this file
                    if let Some(existing_import) = model_type_alias.imports.get_mut(&import_path) {
                        // Add type import for the interface
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: member.ts_name.clone(),
                            alias: None,
                            is_type: true,
                        });
                        // Add instanceOf, FromJSONTyped, and ToJSON functions to existing import
                        // ApiImportStatements automatically handles deduplication and sorting
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: format!("instanceOf{}", member.ts_name),
                            alias: None,
                            is_type: false,
                        });
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: format!("{}FromJSONTyped", member.ts_name),
                            alias: None,
                            is_type: false,
                        });
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: format!("{}ToJSON", member.ts_name),
                            alias: None,
                            is_type: false,
                        });
                    } else {
                        // Create new import for type, instanceOf, FromJSONTyped, and ToJSON functions
                        let func_import = ApiImportStatement::new(import_path.clone())
                            .with_type_import(member.ts_name.clone(), None)
                            .with_import(format!("instanceOf{}", member.ts_name), None)
                            .with_import(format!("{}FromJSONTyped", member.ts_name), None)
                            .with_import(format!("{}ToJSON", member.ts_name), None);
                        model_type_alias.imports.insert(import_path, func_import);
                    }
                }
            }
        }

        // Add imports for FromJSONTyped and ToJSONTyped functions for intersection members
        let intersection_members: Vec<_> = model_type_alias
            .intersection_members()
            .map(|members| members.to_vec())
            .unwrap_or_default();

        for member in &intersection_members {
            if member.is_reference {
                // Find the file for this reference member type
                let member_type_def = schemas
                    .values()
                    .find(|type_def| type_def.ts_name() == member.ts_name);

                if let Some(member_type_def) = member_type_def {
                    let member_filename = self.generate_filename(member_type_def.ts_name());
                    let import_path = format!("./{}", member_filename.trim_end_matches(".ts"));

                    // Find or create the import statement for this file
                    if let Some(existing_import) = model_type_alias.imports.get_mut(&import_path) {
                        // Add type import for the reference
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: member.ts_name.clone(),
                            alias: None,
                            is_type: true,
                        });
                        // Add FromJSONTyped and ToJSONTyped functions to existing import
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: format!("{}FromJSONTyped", member.ts_name),
                            alias: None,
                            is_type: false,
                        });
                        existing_import.imports.insert(ApiImportSpecifier {
                            name: format!("{}ToJSONTyped", member.ts_name),
                            alias: None,
                            is_type: false,
                        });
                    } else {
                        // Create new import for type, FromJSONTyped, and ToJSONTyped functions
                        let func_import = ApiImportStatement::new(import_path.clone())
                            .with_type_import(member.ts_name.clone(), None)
                            .with_import(format!("{}FromJSONTyped", member.ts_name), None)
                            .with_import(format!("{}ToJSONTyped", member.ts_name), None);
                        model_type_alias.imports.insert(import_path, func_import);
                    }
                }
            } else if member.is_object {
                // For object members, extract reference types from properties
                if let Some(properties) = &member.object_properties {
                    for prop in properties {
                        // Add imports for nullable reference
                        if let Some(ref_name) = &prop.nullable_reference_name {
                            let member_type_def = schemas
                                .values()
                                .find(|type_def| type_def.ts_name() == ref_name);

                            if let Some(member_type_def) = member_type_def {
                                let member_filename =
                                    self.generate_filename(member_type_def.ts_name());
                                let import_path =
                                    format!("./{}", member_filename.trim_end_matches(".ts"));

                                if let Some(existing_import) =
                                    model_type_alias.imports.get_mut(&import_path)
                                {
                                    existing_import.imports.insert(ApiImportSpecifier {
                                        name: format!("{}FromJSON", ref_name),
                                        alias: None,
                                        is_type: false,
                                    });
                                    existing_import.imports.insert(ApiImportSpecifier {
                                        name: format!("{}ToJSON", ref_name),
                                        alias: None,
                                        is_type: false,
                                    });
                                } else {
                                    let func_import = ApiImportStatement::new(import_path.clone())
                                        .with_import(format!("{}FromJSON", ref_name), None)
                                        .with_import(format!("{}ToJSON", ref_name), None);
                                    model_type_alias.imports.insert(import_path, func_import);
                                }
                            }
                        }
                        // Add imports for non-nullable reference
                        if let Some(ref_name) = &prop.reference_name {
                            let member_type_def = schemas
                                .values()
                                .find(|type_def| type_def.ts_name() == ref_name);

                            if let Some(member_type_def) = member_type_def {
                                let member_filename =
                                    self.generate_filename(member_type_def.ts_name());
                                let import_path =
                                    format!("./{}", member_filename.trim_end_matches(".ts"));

                                if let Some(existing_import) =
                                    model_type_alias.imports.get_mut(&import_path)
                                {
                                    existing_import.imports.insert(ApiImportSpecifier {
                                        name: format!("{}FromJSON", ref_name),
                                        alias: None,
                                        is_type: false,
                                    });
                                    existing_import.imports.insert(ApiImportSpecifier {
                                        name: format!("{}ToJSON", ref_name),
                                        alias: None,
                                        is_type: false,
                                    });
                                } else {
                                    let func_import = ApiImportStatement::new(import_path.clone())
                                        .with_import(format!("{}FromJSON", ref_name), None)
                                        .with_import(format!("{}ToJSON", ref_name), None);
                                    model_type_alias.imports.insert(import_path, func_import);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(model_type_alias)
    }

    /// Generate apis/index.ts file
    fn generate_apis_index_file(
        &self,
        openapi: &OpenApiV31Spec,
        api_classes: &HashMap<String, FileInfo>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        let mut sorted_api_vec: Vec<(&String, &FileInfo)> = api_classes.iter().collect();
        sorted_api_vec.sort_by(|a, b| a.0.cmp(b.0));
        for (_, file_info) in sorted_api_vec {
            let import_name = file_info.filename.trim_end_matches(".ts");
            exports.push(format!("export * from './{}';", import_name));
        }

        let common_file_header = CommonFileHeaderData::from(HeaderData::from_openapi(openapi));
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

    /// Generate models/index.ts file
    fn generate_models_index_file(
        &self,
        openapi: &OpenApiV31Spec,
        schemas: &HashMap<String, TsTypeDefinition>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        // Collect and sort by actual type names
        let mut type_names: Vec<String> = schemas
            .values()
            .map(|def| def.ts_name().to_string())
            .collect();
        type_names.sort();
        for type_name in type_names {
            let filename = self.generate_filename(&type_name);
            let import_name = filename.trim_end_matches(".ts");
            exports.push(format!("export * from './{}';", import_name));
        }

        let common_file_header = CommonFileHeaderData::from(HeaderData::from_openapi(openapi));
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

    /// Generate main index.ts file
    fn generate_main_index_file(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = vec!["export * from './runtime/runtime';".to_string()];

        // Only export from './apis' if there are paths in the OpenAPI spec
        if let Some(paths) = &openapi.paths
            && !paths.is_empty()
        {
            exports.push("export * from './apis';".to_string());
        }

        // Only export from './models' if there are schemas in the OpenAPI spec
        if let Some(components) = &openapi.components
            && !components.schemas.is_empty()
        {
            exports.push("export * from './models';".to_string());
        }

        let common_file_header = CommonFileHeaderData::from(HeaderData::from_openapi(openapi));
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

    /// Generate package files (package.json, tsconfig.json, etc.)
    fn generate_package_files(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, GeneratorError> {
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

    // =========================================================================
    // IR-based generation
    // =========================================================================

    /// Generate all files from a version-agnostic IR spec.
    ///
    /// This is the new entry point that replaces `CodeGenerator::generate()`.
    /// Currently handles model generation via the IR pipeline; API generation
    /// still falls through to the legacy path (takes `&OpenApiV31Spec`).
    pub fn generate_models_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let output = IrSchemaGenerator::generate(&ir.schemas);
        let schemas = output.type_definitions;
        let enum_discriminators = output.enum_discriminators;

        // Only generate model files for component schemas (not promoted inline schemas)
        // Schemas generated by IrSchemaGenerator (wrapper interfaces, Kind types) that
        // don't appear in ir.schemas are always included.
        let promoted_names: std::collections::HashSet<String> = ir
            .schemas
            .iter()
            .filter(|(_, s)| !s.is_component)
            .map(|(name, _)| name.to_pascal_case())
            .collect();

        // Filter schemas to only those we should generate files for
        let schemas: HashMap<String, TsTypeDefinition> = schemas
            .into_iter()
            .filter(|(name, _)| !promoted_names.contains(name))
            .collect();

        let common_file_header = CommonFileHeaderData::new(
            ir.info.title.clone(),
            ir.info.description.clone(),
            ir.info.version.clone(),
        );

        let mut files = Vec::new();

        // Generate model files (same rendering logic as legacy generate_models)
        for (name, type_def) in &schemas {
            let common_file_header = common_file_header.clone();
            let filename = self.generate_filename(type_def.ts_name());

            // Collect referenced types for this model
            let referenced_types = type_def.referenced_types();

            // Build import statements for referenced types
            let mut imports = ApiImportStatements::new();
            let is_intersection_type = type_def.is_intersection_type();

            for ref_type_name in &referenced_types {
                if ref_type_name == name {
                    continue;
                }

                let ref_type_def = schemas
                    .values()
                    .find(|td| td.ts_name() == ref_type_name);

                if let Some(ref_type_def) = ref_type_def {
                    let actual_type_name = ref_type_def.ts_name();
                    let ref_filename = self.generate_filename(actual_type_name);
                    let import_path = format!("./{}", ref_filename.trim_end_matches(".ts"));

                    let import_stmt = imports
                        .entry(import_path.clone())
                        .or_insert_with(|| ApiImportStatement::new(import_path.clone()));

                    import_stmt.imports.insert(ApiImportSpecifier {
                        name: actual_type_name.to_string(),
                        alias: None,
                        is_type: true,
                    });

                    if !is_intersection_type {
                        for func_name in &[
                            format!("{}FromJSON", actual_type_name),
                            format!("{}FromJSONTyped", actual_type_name),
                            format!("{}ToJSON", actual_type_name),
                        ] {
                            import_stmt.imports.insert(ApiImportSpecifier {
                                name: func_name.clone(),
                                alias: None,
                                is_type: false,
                            });
                        }

                        if let TsTypeDefinition::Interface(_) = ref_type_def {
                            import_stmt.imports.insert(ApiImportSpecifier {
                                name: format!("instanceOf{}", actual_type_name),
                                alias: None,
                                is_type: false,
                            });
                        }
                    }
                }
            }

            // Render model file
            let file = match type_def {
                TsTypeDefinition::Interface(interface) => {
                    let mut model_interface_data = ModelInterfaceData::from_interface(interface);
                    model_interface_data.imports = imports;
                    let ts_name = type_def.ts_name();
                    model_interface_data.update_enum_discriminators(ts_name, &enum_discriminators);

                    let template_context = minijinja::context! {
                        common_file_header,
                        model_interface => model_interface_data,
                    };
                    self.templating
                        .render_template(TemplateName::ModelInterface, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelInterfaceGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
                TsTypeDefinition::TypeAlias(type_alias) => {
                    let model_type_alias =
                        self.create_model_type_alias_data(type_alias, imports, &schemas)?;
                    let template_context = minijinja::context! {
                        common_file_header,
                        model_type_alias,
                    };
                    self.templating
                        .render_template(TemplateName::ModelTypeAlias, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelTypeAliasGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
                TsTypeDefinition::Enum(enum_def) => {
                    let model_enum = ModelEnumData {
                        enum_definition: enum_def.clone(),
                        imports: Vec::new(),
                    };
                    let template_context = minijinja::context! {
                        common_file_header,
                        model_enum,
                    };
                    self.templating
                        .render_template(TemplateName::ModelEnum, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelEnumGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
            };

            files.push(file);
        }

        // Generate models/index.ts
        if !schemas.is_empty() {
            files.push(self.generate_models_index_file_standalone(&common_file_header, &schemas)?);
        }

        Ok(files)
    }

    /// Generate models/index.ts without needing &OpenApiV31Spec.
    fn generate_models_index_file_standalone(
        &self,
        common_file_header: &CommonFileHeaderData,
        schemas: &HashMap<String, TsTypeDefinition>,
    ) -> Result<FileInfo, GeneratorError> {
        let mut exports = Vec::new();

        let mut type_names: Vec<String> = schemas
            .values()
            .map(|def| def.ts_name().to_string())
            .collect();
        type_names.sort();
        for type_name in type_names {
            let filename = self.generate_filename(&type_name);
            let import_name = filename.trim_end_matches(".ts");
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
                "models/index.ts",
                template_context,
            )
            .map_err(|e| GeneratorError::IndexFileGeneration {
                file_path: "models/index.ts".to_string(),
                source: Box::new(e),
            })
    }

    /// Generate ALL files from the IR spec, using the legacy spec only for API generation.
    ///
    /// This is the main entry point for the IR pipeline. It produces:
    /// - Model files from `IrSpec.schemas` (via `IrSchemaGenerator`)
    /// - API files from legacy spec (not yet migrated to IR)
    /// - Runtime, project, readme files from `IrSpec.info`/`IrSpec.servers`
    pub fn generate_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        // --- APIs (legacy path, not yet migrated to IR) ---
        let all_api_data = self
            .collect_operations_by_tag(openapi)
            .into_values()
            .flatten()
            .map(|op_info| op_info.to_api_method_data(openapi.components.as_ref()))
            .collect::<Vec<_>>();
        files.extend(self.generate_apis(openapi, all_api_data)?);

        // --- Models (IR path) ---
        files.extend(self.generate_models_from_ir(ir)?);

        // --- Runtime (IR path) ---
        let common_file_header = CommonFileHeaderData::new(
            ir.info.title.clone(),
            ir.info.description.clone(),
            ir.info.version.clone(),
        );
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
    fn generate_package_json_from_ir(
        &self,
        ir: &openapi_nexus_ir::types::IrSpec,
    ) -> FileInfo {
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

    /// Legacy generation path (mirrors the default `CodeGenerator::generate` implementation).
    /// Used as fallback when IR lowering is unavailable.
    fn generate_legacy(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        let all_api_data = self
            .collect_operations_by_tag(openapi)
            .into_values()
            .flatten()
            .map(|op_info| op_info.to_api_method_data(openapi.components.as_ref()))
            .collect::<Vec<_>>();
        files.extend(self.generate_apis(openapi, all_api_data)?);

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

        let runtime_data = RuntimeData::from_openapi(openapi);
        files.extend(self.generate_runtime(openapi, runtime_data)?);

        files.extend(self.generate_readme(openapi, self.extract_readme_data(openapi))?);
        files.extend(self.generate_project_files(openapi)?);

        Ok(files)
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
        // Use IR pipeline by default. Set OPENAPI_NEXUS_NO_IR=1 to fall back
        // to the legacy path for debugging/comparison.
        let use_ir = std::env::var("OPENAPI_NEXUS_NO_IR").is_err();

        if use_ir {
            match openapi_nexus_ir::lower::v31::lower_v31(openapi) {
                Ok(ir) => {
                    tracing::info!(
                        "TypeScript generator using IR pipeline ({} schemas, {} operations)",
                        ir.schemas.len(),
                        ir.operations.len()
                    );
                    return self.generate_from_ir(&ir, openapi);
                }
                Err(e) => {
                    tracing::warn!(
                        "IR lowering failed, falling back to legacy pipeline: {}",
                        e
                    );
                }
            }
        }

        self.generate_legacy(openapi)
    }

    fn generate_apis(
        &self,
        openapi: &OpenApiV31Spec,
        _apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let operations_by_tag = self.collect_operations_by_tag(openapi);
        let header_data = HeaderData::from_openapi(openapi);
        let common_file_header = CommonFileHeaderData::from(header_data);

        let mut api_classes_map = HashMap::new();
        let mut files = Vec::new();

        // Generate API class files
        let components = openapi.components.as_ref();
        for (tag, operations) in operations_by_tag {
            if !operations.is_empty() {
                let file_info = self
                    .api_operation_generator
                    .generate_api_class(
                        &tag,
                        &operations,
                        &self.templating,
                        &common_file_header,
                        components,
                    )
                    .map_err(|e| GeneratorError::ApiClassGenerationForTag {
                        tag: tag.clone(),
                        source: Box::new(e),
                    })?;
                api_classes_map.insert(tag, file_info);
            }
        }

        // Add API class files
        files.extend(api_classes_map.values().cloned());

        // Generate apis/index.ts file
        if !api_classes_map.is_empty() {
            files.push(self.generate_apis_index_file(openapi, &api_classes_map)?);
        }

        Ok(files)
    }

    fn generate_models(
        &self,
        openapi: &OpenApiV31Spec,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        // Note: Duplicate schema name checking is performed in generate_apis
        // to catch conflicts early in the generation process

        let (schemas, enum_discriminators) = if let Some(components) = &openapi.components {
            self.generate_model_type_definitions(models, components)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let common_file_header = CommonFileHeaderData::from(HeaderData::from_openapi(openapi));
        let mut files = Vec::new();

        // Generate model files
        for (name, type_def) in &schemas {
            let common_file_header = common_file_header.clone();
            let filename = self.generate_filename(type_def.ts_name());

            // Collect referenced types for this model
            let referenced_types = type_def.referenced_types();

            // Build import statements for referenced types
            // BTreeMap ensures stable ordering, BTreeSet in ApiImportStatement ensures sorted/deduplicated specifiers
            let mut imports = ApiImportStatements::new();

            // For intersection types (allOf), we handle imports separately in create_model_type_alias_data
            // to only import what we actually use (FromJSONTyped/ToJSONTyped)
            let is_intersection_type = type_def.is_intersection_type();

            for ref_type_name in &referenced_types {
                // Skip self-reference
                if ref_type_name == name {
                    continue;
                }

                // Find the schema where ts_name() matches the referenced type name
                // Note: schemas HashMap is keyed by original OpenAPI names (e.g., "AZ"),
                // but ref_type_name is PascalCase (e.g., "Az"), so we need to search by ts_name()
                let ref_type_def = schemas
                    .values()
                    .find(|type_def| type_def.ts_name() == ref_type_name);

                if let Some(ref_type_def) = ref_type_def {
                    // Use the actual name from the type definition
                    let actual_type_name = ref_type_def.ts_name();
                    let ref_filename = self.generate_filename(actual_type_name);
                    let import_path = format!("./{}", ref_filename.trim_end_matches(".ts"));

                    // Get or create the import statement for this file
                    let import_stmt = imports
                        .entry(import_path.clone())
                        .or_insert_with(|| ApiImportStatement::new(import_path.clone()));

                    import_stmt.imports.insert(ApiImportSpecifier {
                        name: actual_type_name.to_string(),
                        alias: None,
                        is_type: true,
                    });

                    // For intersection types, skip general function imports - they're handled separately
                    // to only import what's actually used (FromJSONTyped/ToJSONTyped)
                    if !is_intersection_type {
                        // All type definitions (Interface, Enum, TypeAlias) have FromJSON/ToJSON/FromJSONTyped functions
                        for func_name in &[
                            format!("{}FromJSON", actual_type_name),
                            format!("{}FromJSONTyped", actual_type_name),
                            format!("{}ToJSON", actual_type_name),
                        ] {
                            import_stmt.imports.insert(ApiImportSpecifier {
                                name: func_name.clone(),
                                alias: None,
                                is_type: false,
                            });
                        }

                        // Interfaces also have instanceOf functions (as values, not types)
                        if let TsTypeDefinition::Interface(_) = ref_type_def {
                            import_stmt.imports.insert(ApiImportSpecifier {
                                name: format!("instanceOf{}", actual_type_name),
                                alias: None,
                                is_type: false,
                            });
                        }
                    }
                }
            }

            // Emit model content using template
            let file = match type_def {
                TsTypeDefinition::Interface(interface) => {
                    // Create ModelInterfaceData from interface definition
                    let mut model_interface_data = ModelInterfaceData::from_interface(interface);
                    model_interface_data.imports = imports;
                    // Update enum discriminators if this is a tagged enum variant
                    // Use the TypeScript name (PascalCase) for lookup
                    let ts_name = type_def.ts_name();
                    model_interface_data.update_enum_discriminators(ts_name, &enum_discriminators);

                    let template_context = minijinja::context! {
                        common_file_header,
                        model_interface => model_interface_data,
                    };
                    self.templating
                        .render_template(TemplateName::ModelInterface, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelInterfaceGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
                TsTypeDefinition::TypeAlias(type_alias) => {
                    let model_type_alias =
                        self.create_model_type_alias_data(type_alias, imports, &schemas)?;
                    let template_context = minijinja::context! {
                        common_file_header,
                        model_type_alias,
                    };
                    self.templating
                        .render_template(TemplateName::ModelTypeAlias, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelTypeAliasGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
                TsTypeDefinition::Enum(enum_def) => {
                    let model_enum = ModelEnumData {
                        enum_definition: enum_def.clone(),
                        imports: Vec::new(),
                    };
                    let template_context = minijinja::context! {
                        common_file_header,
                        model_enum,
                    };
                    self.templating
                        .render_template(TemplateName::ModelEnum, &filename, template_context)
                        .map_err(|e| GeneratorError::ModelEnumGeneration {
                            model_name: name.clone(),
                            source: Box::new(e),
                        })?
                }
            };

            files.push(file);
        }

        // Generate models/index.ts file only if there are schemas
        if !schemas.is_empty() {
            files.push(self.generate_models_index_file(openapi, &schemas)?);
        }

        Ok(files)
    }

    fn generate_runtime(
        &self,
        openapi: &OpenApiV31Spec,
        runtime_data: RuntimeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header_data = HeaderData::from_openapi(openapi);
        let common_file_header = CommonFileHeaderData::from(header_data);
        let runtime_runtime = RuntimeRuntimeData::from(runtime_data);
        let template_context = minijinja::context! {
            common_file_header,
            runtime_runtime,
        };
        let file = self
            .templating
            .render_template(TemplateName::Runtime, "runtime.ts", template_context)
            .map_err(|e| GeneratorError::RuntimeTemplate {
                source: Box::new(e),
            })?;

        Ok(vec![file])
    }

    fn generate_project_files(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = self.generate_package_files(openapi)?;
        files.push(self.generate_main_index_file(openapi)?);
        Ok(files)
    }

    fn generate_readme(
        &self,
        _: &OpenApiV31Spec,
        data: openapi_nexus_core::data::ReadmeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let file = self.templating.render_template(
            TemplateName::Readme,
            "README.md",
            minijinja::Value::from_serialize(data),
        )?;
        Ok(vec![file])
    }
}

impl FileWriter for TypeScriptFetchCodeGenerator {}
