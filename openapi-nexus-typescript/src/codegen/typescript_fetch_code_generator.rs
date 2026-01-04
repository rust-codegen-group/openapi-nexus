//! TypeScript Fetch code generator

use std::collections::{HashMap, HashSet};
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};
use utoipa::openapi;
use utoipa::openapi::OpenApi;

use crate::ast::{TsTypeAliasDefinition, TsTypeDefinition};
use crate::config::TypeScriptFetchConfig;
use crate::errors::GeneratorError;
use crate::generator::{
    api_operation_generator::ApiOperationGenerator, package_files_generator::PackageFilesGenerator,
    schema_context::SchemaContext, schema_generator::SchemaGenerator,
};
use crate::templating::data::{
    ApiImportSpecifier, ApiImportStatement, ApiImportStatements, CommonFileHeaderData,
    ModelEnumData, ModelInterfaceData, ModelTypeAliasData, ProjectIndexData, RuntimeRuntimeData,
};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::NamingConvention;
use openapi_nexus_core::data::{ApiMethodData, HeaderData, ModelData, RuntimeData};
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};

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
        components: &openapi::Components,
    ) -> (HashMap<String, TsTypeDefinition>, HashMap<String, (String, String)>) {
        let mut schemas = HashMap::new();
        let mut visited = HashSet::new();
        let mut inline_interfaces = HashMap::new();
        let mut enum_discriminators = HashMap::new();
        let mut context =
            SchemaContext::new(&components.schemas, &mut visited, &mut inline_interfaces, &mut enum_discriminators);

        for model in models {
            let type_def = self.schema_generator.schema_to_ts_type_definition(
                &model.name,
                &model.schema,
                &mut context,
            );
            schemas.insert(model.name, type_def);
        }

        // Collect all generated inline interfaces and add them to schemas
        // Use the original_name from the type definition as the key for consistency
        for type_def in context.get_inline_interfaces().values() {
            let original_name = type_def.original_name().to_string();
            schemas.insert(original_name, type_def.clone());
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
            if member.is_interface {
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
        openapi: &OpenApi,
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
        openapi: &OpenApi,
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
    fn generate_main_index_file(&self, openapi: &OpenApi) -> Result<FileInfo, GeneratorError> {
        let mut exports = vec!["export * from './runtime/runtime';".to_string()];

        // Only export from './apis' if there are paths in the OpenAPI spec
        if !openapi.paths.paths.is_empty() {
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
    fn generate_package_files(&self, openapi: &OpenApi) -> Result<Vec<FileInfo>, GeneratorError> {
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

impl CodeGenerator for TypeScriptFetchCodeGenerator {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::TypeScriptFetch
    }

    fn generate_apis(
        &self,
        openapi: &OpenApi,
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
        openapi: &OpenApi,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
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
                    model_interface_data.update_enum_discriminators(&ts_name, &enum_discriminators);

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
        openapi: &OpenApi,
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
        openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = self.generate_package_files(openapi)?;
        files.push(self.generate_main_index_file(openapi)?);
        Ok(files)
    }

    fn generate_readme(
        &self,
        _: &OpenApi,
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
