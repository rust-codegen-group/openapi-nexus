//! Go HTTP code generator

use std::collections::BTreeMap;
use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};
use minijinja::context;
use tracing::warn;

use crate::ast::GoStruct;
use crate::ast::ty::GoTypeDefinition;
use crate::config::GoHttpConfig;
use crate::consts::{MAX_LINE_WIDTH, escape_go_keyword};
use crate::errors::GeneratorError;
use crate::templating::data::{
    ApiOperationData, CommonFileHeaderData, GoApiMethodData, GoParameterInfo, MainSdkData,
    ModelStructData, ModelTypeAliasData, OperationResponse, OperationsData, SubClientInfo,
};
use crate::templating::{TemplateName, Templates};
use crate::type_mapping;
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::data::{
    ApiMethodData, HeaderData, ModelData, OperationInfo, ParameterInfo, collect_operations_by_tag,
};
use openapi_nexus_core::traits::OpenApiRefExt as _;
use openapi_nexus_core::traits::ToRcDoc;
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};
use openapi_nexus_ir::types::IrSpec;
use openapi_nexus_spec::OpenApiV31Spec;
use openapi_nexus_spec::oas31::spec::{Components, ObjectOrReference};

/// Go HTTP code generator
#[derive(Debug, Clone)]
pub struct GoHttpCodeGenerator {
    config: GoHttpConfig,
    templates: Templates,
}

impl GoHttpCodeGenerator {
    /// Create a new Go HTTP generator
    ///
    /// # Arguments
    /// * `config` - TOML config table
    pub fn new(config: toml::value::Table) -> Self {
        let parsed_config = GoHttpConfig::from(config);
        let templates = Templates::new();
        Self {
            config: parsed_config,
            templates,
        }
    }

    /// Generate filename based on naming convention
    fn generate_filename(&self, name: &str) -> String {
        let base_name = match self.config.file_naming_convention {
            openapi_nexus_core::NamingConvention::CamelCase => name.to_lower_camel_case(),
            openapi_nexus_core::NamingConvention::KebabCase => name.to_kebab_case(),
            openapi_nexus_core::NamingConvention::SnakeCase => name.to_snake_case(),
            openapi_nexus_core::NamingConvention::PascalCase => name.to_pascal_case(),
        };
        // Escape reserved keywords in filename to avoid issues
        let escaped_name = escape_go_keyword(&base_name);

        // Avoid creating filenames ending with _test.go as Go treats them as test files
        // and excludes them from regular builds. Append _type to avoid this issue.
        let final_name = if escaped_name.ends_with("_test") {
            format!("{}_type", escaped_name)
        } else {
            escaped_name
        };

        format!("{}.go", final_name)
    }

    /// Convert ParameterInfo to GoParameterInfo
    fn convert_parameter(
        &self,
        param: &ParameterInfo,
        components: Option<&Components>,
    ) -> Result<GoParameterInfo, GeneratorError> {
        // Convert schema to Go type
        let go_type = if let Some(schema_ref) = &param.schema {
            let go_expr = type_mapping::schema_to_go_expression(schema_ref, components)?;
            // Convert GoExpression to string
            let doc = go_expr.to_rcdoc();
            doc.pretty(MAX_LINE_WIDTH).to_string().trim().to_string()
        } else {
            "string".to_string() // Default to string if no schema
        };

        let param_name_pascal = escape_go_keyword(&param.param_name.to_pascal_case());
        let param_name_camel = escape_go_keyword(&param.param_name.to_lower_camel_case());

        Ok(GoParameterInfo {
            original_name: param.original_name.clone(),
            param_name: param_name_pascal,
            param_name_camel,
            go_type,
            required: param.required,
            description: param.description.clone(),
        })
    }

    /// Get module path from config or return default
    fn get_module_path(&self) -> String {
        self.config
            .module_path
            .clone()
            .unwrap_or_else(|| "example.com/sdk".to_string())
    }

    /// Group APIs by their tags based on matching operations
    fn group_apis_by_tag<'a>(
        &self,
        apis: &'a [ApiMethodData],
        operations_by_tag: &std::collections::HashMap<String, Vec<OperationInfo>>,
    ) -> BTreeMap<String, Vec<&'a ApiMethodData>> {
        let mut apis_by_tag: BTreeMap<String, Vec<&'a ApiMethodData>> = BTreeMap::new();
        for api in apis {
            // Extract tag from operation (we'll need to match by path/method)
            // For now, group by finding matching operations
            for (tag, operations) in operations_by_tag {
                for op_info in operations {
                    if op_info.path == api.path && op_info.method == api.http_method {
                        apis_by_tag.entry(tag.clone()).or_default().push(api);
                        break;
                    }
                }
            }
        }
        apis_by_tag
    }

    /// Convert a single ApiMethodData to GoApiMethodData
    fn convert_api_to_go_method(
        &self,
        api: &ApiMethodData,
        operations: &[OperationInfo],
        components: Option<&Components>,
    ) -> Result<GoApiMethodData, GeneratorError> {
        // Find matching operation for additional details
        let op_info = operations
            .iter()
            .find(|op| op.path == api.path && op.method == api.http_method);

        // Convert method name to PascalCase for Go
        let method_name = api.method_name.to_pascal_case();

        // Convert parameters
        let path_params: Result<Vec<GoParameterInfo>, GeneratorError> = api
            .path_params
            .iter()
            .map(|p| self.convert_parameter(p, components))
            .collect();
        let query_params: Result<Vec<GoParameterInfo>, GeneratorError> = api
            .query_params
            .iter()
            .map(|p| self.convert_parameter(p, components))
            .collect();
        let header_params: Result<Vec<GoParameterInfo>, GeneratorError> = api
            .header_params
            .iter()
            .map(|p| self.convert_parameter(p, components))
            .collect();

        // Extract request body content type
        let request_body_content_type = api
            .request_body
            .as_ref()
            .and_then(|rb_ref| match rb_ref {
                ObjectOrReference::Object(rb) => Some(rb),
                ObjectOrReference::Ref { .. } => None,
            })
            .and_then(|rb| {
                rb.content
                    .get("application/json")
                    .map(|_| "application/json")
                    .or_else(|| rb.content.keys().next().map(|k| k.as_str()))
            })
            .unwrap_or("application/json")
            .to_string();

        // Extract request body type name
        let request_body_type = api
            .request_body
            .as_ref()
            .and_then(|rb_ref| match rb_ref {
                ObjectOrReference::Object(rb) => Some(rb),
                ObjectOrReference::Ref { .. } => None,
            })
            .and_then(|rb| rb.content.get("application/json"))
            .and_then(|json_content| json_content.schema.as_ref())
            .map(|schema_ref| match schema_ref {
                ObjectOrReference::Object(_) => format!("{}Request", method_name),
                ObjectOrReference::Ref { .. } => schema_ref
                    .schema_name()
                    .map(|n| n.to_pascal_case())
                    .unwrap_or_else(|| format!("{}Request", method_name)),
            });

        Ok(GoApiMethodData {
            name: method_name.clone(),
            http_method: api.http_method.as_str().to_uppercase(),
            path: api.path.clone(),
            operation_id: op_info
                .and_then(|op| op.operation.operation_id.clone())
                .unwrap_or_else(|| method_name.clone()),
            path_params: path_params?,
            query_params: query_params?,
            header_params: header_params?,
            body_param: None, // Request body is handled as a separate parameter
            has_request_body: api.request_body.is_some(),
            request_body_content_type,
            request_body_type,
            response_type: None, // TODO: Extract from return_type
            description: op_info.and_then(|op| op.operation.description.clone()),
        })
    }

    /// Build import list for API client files
    fn build_api_imports(&self, go_methods: &[GoApiMethodData], module_path: &str) -> Vec<String> {
        // Check if any method needs io import (when has_request_body is false)
        let needs_io_import = go_methods.iter().any(|method| !method.has_request_body);

        // Collect and separate std imports from project imports
        let mut std_imports = vec![
            "context".to_string(),
            "fmt".to_string(),
            "net/http".to_string(),
            "net/url".to_string(),
        ];
        if needs_io_import {
            std_imports.push("io".to_string());
        }
        std_imports.sort();

        let mut project_imports = vec![
            format!("{}/internal/config", module_path),
            format!("{}/internal/hooks", module_path),
            format!("{}/internal/utils", module_path),
            format!("{}/models/components", module_path),
            format!("{}/models/operations", module_path),
        ];

        project_imports.sort();

        // Combine: std imports, empty string separator, project imports
        let mut imports = std_imports;
        imports.push(String::new()); // Empty string as separator for newline
        imports.extend(project_imports);
        imports
    }

    /// Generate API client file for a single tag
    fn generate_api_client_file(
        &self,
        tag: &str,
        operations: &[OperationInfo],
        tag_apis: &[&ApiMethodData],
        openapi: &OpenApiV31Spec,
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<FileInfo, Box<dyn Error + Send + Sync>> {
        // Convert core ApiMethodData to Go-specific GoApiMethodData
        let components = openapi.components.as_ref();
        let go_methods: Result<Vec<GoApiMethodData>, GeneratorError> = tag_apis
            .iter()
            .map(|api| self.convert_api_to_go_method(api, operations, components))
            .collect();

        let go_methods = go_methods?;

        // Create client struct (escape reserved keywords)
        let client_struct = GoStruct::new(escape_go_keyword(&tag.to_pascal_case()));

        // Get SDK name from OpenAPI title
        let sdk_name = openapi.info.title.to_pascal_case();

        // Build imports
        let imports = self.build_api_imports(&go_methods, module_path);

        let api_data = ApiOperationData::new(
            client_struct,
            escape_go_keyword(tag),
            sdk_name,
            common_header.clone(),
        )
        .with_methods(go_methods)
        .with_imports(imports);

        // Wrap in context with api_operation key (matching template expectations)
        let template_context = context! {
            api_operation => api_data,
            common_file_header => common_header,
            module_path => module_path,
        };
        let filename = self.generate_filename(tag);
        let file_info = self.templates.render_template(
            TemplateName::ApiOperation,
            &filename,
            template_context,
        )?;
        Ok(file_info)
    }

    /// Generate operations types file
    fn generate_operations_file(
        &self,
        apis: &[ApiMethodData],
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<FileInfo, Box<dyn Error + Send + Sync>> {
        let mut responses = Vec::new();
        for api in apis {
            let method_name = api.method_name.to_pascal_case();
            responses.push(OperationResponse {
                name: format!("{}Response", method_name),
                operation_name: method_name.clone(),
                body_type: None, // TODO: Extract from return_type
            });
        }
        // Sort responses by name to ensure stable ordering across generations
        responses.sort_by(|a, b| a.name.cmp(&b.name));
        let operations_data =
            OperationsData::new(responses, common_header.clone(), module_path.to_string());
        let operations_context = context! {
            operations => operations_data,
            common_file_header => common_header,
            module_path => module_path,
        };
        let operations_file = self.templates.render_template(
            TemplateName::ModelOperations,
            "operations/operations.go",
            operations_context,
        )?;
        Ok(operations_file)
    }

    /// Generate main SDK file
    fn generate_main_sdk_file(
        &self,
        openapi: &OpenApiV31Spec,
        apis_by_tag: &BTreeMap<String, Vec<&ApiMethodData>>,
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<FileInfo, Box<dyn Error + Send + Sync>> {
        let sdk_name: String = openapi.info.title.to_pascal_case();
        let package_name: String = self
            .config
            .package_name
            .as_ref()
            .map(|s| s.to_snake_case())
            .unwrap_or_else(|| "sdk".to_string());

        // Collect sub-clients from all tags
        let mut sub_clients: Vec<SubClientInfo> = Vec::new();
        for tag in apis_by_tag.keys() {
            let client_name = escape_go_keyword(&tag.to_pascal_case());
            sub_clients.push(SubClientInfo {
                name: escape_go_keyword(&tag.to_lower_camel_case()),
                type_name: client_name.clone(),
            });
        }

        let main_sdk_data = MainSdkData::new(sdk_name.clone(), package_name, common_header.clone())
            .with_sub_clients(sub_clients);
        let template_context = context! {
            main_sdk => main_sdk_data,
            common_file_header => common_header,
            module_path => module_path,
        };
        // SDK file goes in sdk/ subdirectory so it can be imported
        let sdk_filename = if let Some(pkg) = &self.config.package_name {
            format!("sdk/{}.go", pkg.to_snake_case())
        } else {
            format!("sdk/{}.go", sdk_name.to_snake_case())
        };
        let file_info = self.templates.render_template(
            TemplateName::MainSdk,
            &sdk_filename,
            template_context,
        )?;
        Ok(file_info)
    }

    /// Build model imports with module path and time import detection
    fn build_model_imports(
        &self,
        imports: &[String],
        module_path: &str,
        type_def: &GoTypeDefinition,
    ) -> Vec<String> {
        // Add module path to imports
        let mut full_imports: Vec<String> = imports
            .iter()
            .map(|imp| {
                if imp.starts_with("optionalnullable") {
                    format!("{}/runtime/{}", module_path, imp)
                } else if imp.starts_with("internal/") {
                    // Internal packages are now at root level (Option B)
                    format!("{}/{}", module_path, imp)
                } else {
                    imp.clone()
                }
            })
            .collect();

        // Check if the type definition uses time types and add time import if needed
        let type_def_str = match type_def {
            GoTypeDefinition::Struct(s) => {
                let doc = s.to_rcdoc();
                doc.pretty(MAX_LINE_WIDTH).to_string()
            }
            GoTypeDefinition::TypeAlias(t) => {
                let doc = t.to_rcdoc();
                doc.pretty(MAX_LINE_WIDTH).to_string()
            }
        };
        if (type_def_str.contains("time.Time") || type_def_str.contains("time.Duration"))
            && !full_imports.iter().any(|imp| imp == "time")
        {
            full_imports.push("time".to_string());
        }

        full_imports
    }

    /// Process a single model and generate its file
    fn process_model(
        &self,
        model: &ModelData,
        components: &Components,
        header_data: &HeaderData,
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<FileInfo, Box<dyn Error + Send + Sync>> {
        let (type_def, imports, required_fields) = type_mapping::generate_model_data(
            &model.name,
            &model.schema,
            components,
            &self.config,
            header_data,
        )
        .map_err(|e| GeneratorError::ModelGeneration {
            model_name: model.name.clone(),
            source: Box::new(e),
        })?;

        // Build imports with module path and time detection
        let full_imports = self.build_model_imports(&imports, module_path, &type_def);

        let filename = format!("components/{}", self.generate_filename(&model.name));
        let (template_context, template_name) = match type_def {
            GoTypeDefinition::Struct(s) => {
                let model_data = ModelStructData::new(s, common_header.clone())
                    .with_imports(full_imports)
                    .with_required_fields(required_fields);
                (
                    context! {
                        model_struct => model_data,
                        common_file_header => common_header,
                    },
                    TemplateName::ModelStruct,
                )
            }
            GoTypeDefinition::TypeAlias(t) => {
                let model_data =
                    ModelTypeAliasData::new(t, common_header.clone()).with_imports(full_imports);
                (
                    context! {
                        model_type_alias => model_data,
                        common_file_header => common_header,
                    },
                    TemplateName::ModelTypeAlias,
                )
            }
        };

        let file_info =
            self.templates
                .render_template(template_name, &filename, template_context)?;
        Ok(file_info)
    }

    /// Generate internal runtime files
    fn generate_internal_runtime_files(
        &self,
        internal_files: &[(&str, TemplateName)],
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();
        for (filename, template_name) in internal_files {
            let template_context = context! {
                common_file_header => common_header,
                module_path => module_path,
            };
            let content = self
                .templates
                .render_template_string(*template_name, template_context)?;
            // Use ProjectFiles category so files go to root level
            files.push(FileInfo::project(filename.to_string(), content));
        }
        Ok(files)
    }

    /// Generate runtime type files
    fn generate_runtime_type_files(
        &self,
        runtime_files: &[(&str, TemplateName)],
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();
        for (filename, template_name) in runtime_files {
            let template_context = context! {
                common_file_header => common_header,
                module_path => module_path,
            };
            let file_info =
                self.templates
                    .render_template(*template_name, filename, template_context)?;
            files.push(file_info);
        }
        Ok(files)
    }

    /// Generate request body models for inline schemas
    fn generate_request_body_models(
        &self,
        apis: &[ApiMethodData],
        components: Option<&Components>,
        header_data: &HeaderData,
        common_header: &CommonFileHeaderData,
        module_path: &str,
    ) -> Vec<FileInfo> {
        let mut files = Vec::new();

        for api in apis {
            let Some(ObjectOrReference::Object(request_body)) = &api.request_body else {
                continue;
            };
            let Some(json_content) = request_body.content.get("application/json") else {
                continue;
            };
            let Some(schema_ref) = &json_content.schema else {
                continue;
            };

            // Only generate a new model if it's an inline schema (not a reference)
            let ObjectOrReference::Object(inline_schema) = schema_ref else {
                continue;
            };

            let method_name = api.method_name.to_pascal_case();
            let request_type_name = format!("{}Request", method_name);

            let Some(components_ref) = components else {
                continue;
            };

            let model_data = ModelData {
                name: request_type_name.clone(),
                schema: ObjectOrReference::Object(inline_schema.clone()),
            };

            if let Ok(file_info) = self.process_model(
                &model_data,
                components_ref,
                header_data,
                common_header,
                module_path,
            ) {
                files.push(file_info);
            } else {
                // Log error but continue - some request bodies might not be generatable
                warn!(
                    request_type_name = %request_type_name,
                    "Failed to generate request body type"
                );
            }
        }

        files
    }
}

impl CodeGenerator for GoHttpCodeGenerator {
    fn language(&self) -> Language {
        Language::Go
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::GoHttp
    }

    fn generate(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let ir = openapi_nexus_ir::lower::v31::lower_v31(openapi)?;
        tracing::info!(
            "Go generator using IR pipeline ({} schemas, {} operations)",
            ir.schemas.len(),
            ir.operations.len()
        );
        self.generate_from_ir(openapi, &ir)
    }
}

impl GoHttpCodeGenerator {
    /// Top-level IR pipeline entry: produce all files for the Go HTTP generator.
    ///
    /// The IR drives spec-level orchestration (info/servers/schema+operation counts),
    /// but the per-schema Go type mapping still consumes raw `OpenApiV31Spec` /
    /// `Components` because `type_mapping::schema_to_go_expression` has not yet been
    /// ported to `IrTypeExpr`. That port is Phase 4 work.
    fn generate_from_ir(
        &self,
        openapi: &OpenApiV31Spec,
        ir: &IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let mut files = Vec::new();

        let apis: Vec<ApiMethodData> = collect_operations_by_tag(openapi)
            .into_values()
            .flatten()
            .map(|op_info| op_info.to_api_method_data(openapi.components.as_ref()))
            .collect();

        let models: Vec<ModelData> = openapi
            .components
            .as_ref()
            .map(|c| {
                c.schemas
                    .iter()
                    .map(|(name, schema_ref)| ModelData {
                        name: name.clone(),
                        schema: schema_ref.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        files.extend(self.generate_apis_phase(openapi, apis)?);
        files.extend(self.generate_models_phase(openapi, models)?);
        files.extend(self.generate_runtime_phase(openapi)?);
        files.extend(self.generate_readme_phase(ir)?);
        files.extend(self.generate_project_files_phase()?);

        Ok(files)
    }

    fn generate_apis_phase(
        &self,
        openapi: &OpenApiV31Spec,
        apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let operations_by_tag = collect_operations_by_tag(openapi);
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data.clone());
        let module_path = self.get_module_path();
        let components = openapi.components.as_ref();

        // Group APIs by tag
        let apis_by_tag = self.group_apis_by_tag(&apis, &operations_by_tag);

        // Generate request body models for inline schemas
        let mut files = self.generate_request_body_models(
            &apis,
            components,
            &header_data,
            &common_header,
            &module_path,
        );

        // Generate API client files for each tag
        for (tag, operations) in operations_by_tag {
            let tag_apis: Vec<&ApiMethodData> = apis_by_tag.get(&tag).cloned().unwrap_or_default();

            let file_info = self.generate_api_client_file(
                &tag,
                &operations,
                &tag_apis,
                openapi,
                &common_header,
                &module_path,
            )?;
            files.push(file_info);
        }

        // Generate operations types file
        files.push(self.generate_operations_file(&apis, &common_header, &module_path)?);

        // Generate main SDK file
        files.push(self.generate_main_sdk_file(
            openapi,
            &apis_by_tag,
            &common_header,
            &module_path,
        )?);

        Ok(files)
    }

    fn generate_models_phase(
        &self,
        openapi: &OpenApiV31Spec,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data.clone());
        let mut files = Vec::new();

        let module_path = self.get_module_path();

        if let Some(components) = &openapi.components {
            for model in models {
                let file_info = self.process_model(
                    &model,
                    components,
                    &header_data,
                    &common_header,
                    &module_path,
                )?;
                files.push(file_info);
            }
        }

        // Always generate HTTPMetadata type in components package (needed by operations)
        let httpmetadata_context = context! {
            common_file_header => common_header.clone(),
        };
        let httpmetadata_file = self.templates.render_template(
            TemplateName::ModelHttpMetadata,
            "components/httpmetadata.go",
            httpmetadata_context,
        )?;
        files.push(httpmetadata_file);

        Ok(files)
    }

    fn generate_runtime_phase(
        &self,
        openapi: &OpenApiV31Spec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data);
        let mut files = Vec::new();

        // Generate runtime utility files
        // Internal runtime files.
        let internal_files = vec![
            ("internal/utils/utils.go", TemplateName::RuntimeUtils),
            (
                "internal/utils/requestbody.go",
                TemplateName::RuntimeRequestBody,
            ),
            (
                "internal/utils/queryparams.go",
                TemplateName::RuntimeQueryParams,
            ),
            (
                "internal/utils/pathparams.go",
                TemplateName::RuntimePathParams,
            ),
            ("internal/utils/headers.go", TemplateName::RuntimeHeaders),
            ("internal/utils/json.go", TemplateName::RuntimeJson),
            (
                "internal/utils/contenttype.go",
                TemplateName::RuntimeContentType,
            ),
            ("internal/utils/form.go", TemplateName::RuntimeForm),
            ("internal/utils/retries.go", TemplateName::RuntimeRetries),
            ("internal/utils/security.go", TemplateName::RuntimeSecurity),
            ("internal/utils/env.go", TemplateName::RuntimeEnv),
            (
                "internal/config/sdkconfiguration.go",
                TemplateName::RuntimeConfig,
            ),
            ("internal/hooks/hooks.go", TemplateName::RuntimeHooks),
            (
                "internal/hooks/registration.go",
                TemplateName::RuntimeHooksRegistration,
            ),
        ];

        let runtime_files = vec![
            ("retry/config.go", TemplateName::RuntimeRetryConfig),
            ("types/pointers.go", TemplateName::TypesPointers),
            ("types/date.go", TemplateName::TypesDate),
            ("types/datetime.go", TemplateName::TypesDateTime),
            ("types/bigint.go", TemplateName::TypesBigInt),
            (
                "optionalnullable/optionalnullable.go",
                TemplateName::TypesOptionalNullable,
            ),
        ];

        let module_path = self.get_module_path();

        // Generate internal files (go to root level with ProjectFiles category)
        let mut internal_file_infos =
            self.generate_internal_runtime_files(&internal_files, &common_header, &module_path)?;
        files.append(&mut internal_file_infos);

        // Generate other runtime files (stay in runtime/ directory)
        let mut runtime_file_infos =
            self.generate_runtime_type_files(&runtime_files, &common_header, &module_path)?;
        files.append(&mut runtime_file_infos);

        Ok(files)
    }

    fn generate_project_files_phase(
        &self,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let module_path = self.get_module_path();

        let template_context = context! {
            module_path => module_path,
        };

        let file_info =
            self.templates
                .render_template(TemplateName::GoMod, "go.mod", template_context)?;

        Ok(vec![file_info])
    }

    fn generate_readme_phase(
        &self,
        ir: &IrSpec,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let default_module = "example.com/sdk".to_string();
        let module_path = self.config.module_path.as_ref().unwrap_or(&default_module);

        let package_name = ir.info.title.to_kebab_case();
        let version = ir.info.version.clone();
        let description = ir
            .info
            .description
            .clone()
            .unwrap_or_else(|| "Generated API client".to_string());

        let template_context = context! {
            package_name => package_name,
            description => description,
            version => version,
            module_path => module_path,
        };

        let file_info =
            self.templates
                .render_template(TemplateName::Readme, "README.md", template_context)?;

        Ok(vec![file_info])
    }
}

impl FileWriter for GoHttpCodeGenerator {}
