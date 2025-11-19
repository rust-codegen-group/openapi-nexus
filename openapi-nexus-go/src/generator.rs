//! Go HTTP code generator

use std::error::Error;

use heck::{ToKebabCase as _, ToLowerCamelCase as _, ToPascalCase as _, ToSnakeCase as _};
use minijinja::context;
use utoipa::openapi::OpenApi;

use crate::ast::GoStruct;
use crate::config::GoHttpConfig;
use crate::errors::GeneratorError;
use crate::templating::data::{
    ApiOperationData, CommonFileHeaderData, GoApiMethodData, GoParameterInfo, MainSdkData,
    ModelStructData, ModelTypeAliasData,
};
use crate::templating::{TemplateName, Templates};
use crate::type_mapping;
use openapi_nexus_common::{GeneratorType, Language};
use openapi_nexus_core::data::{
    ApiMethodData, HeaderData, ModelData, ParameterInfo, ReadmeData, RuntimeData,
};
use openapi_nexus_core::traits::ToRcDoc;
use openapi_nexus_core::traits::code_generator::CodeGenerator;
use openapi_nexus_core::traits::file_writer::{FileInfo, FileWriter};

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

        format!("{}.go", base_name)
    }

    /// Convert ParameterInfo to GoParameterInfo
    fn convert_parameter(
        &self,
        param: &ParameterInfo,
        components: Option<&utoipa::openapi::Components>,
    ) -> Result<GoParameterInfo, GeneratorError> {
        // Convert schema to Go type
        let go_type = if let Some(schema_ref) = &param.schema {
            let go_expr = type_mapping::schema_to_go_expression(schema_ref, components)?;
            // Convert GoExpression to string
            let doc = go_expr.to_rcdoc();
            doc.pretty(80).to_string().trim().to_string()
        } else {
            "string".to_string() // Default to string if no schema
        };

        Ok(GoParameterInfo {
            original_name: param.original_name.clone(),
            param_name: param.param_name.to_pascal_case(),
            param_name_camel: param.param_name.to_lower_camel_case(),
            go_type,
            required: param.required,
            description: param.description.clone(),
        })
    }
}

impl CodeGenerator for GoHttpCodeGenerator {
    fn language(&self) -> Language {
        Language::Go
    }

    fn generator_type(&self) -> GeneratorType {
        GeneratorType::GoHttp
    }

    fn generate_apis(
        &self,
        openapi: &OpenApi,
        apis: Vec<ApiMethodData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let operations_by_tag = self.collect_operations_by_tag(openapi);
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data);
        let mut files = Vec::new();

        let module_path = self
            .config
            .module_path
            .clone()
            .unwrap_or_else(|| "example.com/sdk".to_string());

        // Group APIs by tag
        let mut apis_by_tag: std::collections::BTreeMap<String, Vec<&ApiMethodData>> =
            std::collections::BTreeMap::new();
        for api in &apis {
            // Extract tag from operation (we'll need to match by path/method)
            // For now, group by finding matching operations
            for (tag, operations) in &operations_by_tag {
                for op_info in operations {
                    if op_info.path == api.path && op_info.method == api.http_method {
                        apis_by_tag.entry(tag.clone()).or_default().push(api);
                        break;
                    }
                }
            }
        }

        // Generate API client files for each tag
        for (tag, operations) in operations_by_tag {
            // Find matching APIs for this tag
            let tag_apis: Vec<&ApiMethodData> = apis_by_tag.get(&tag).cloned().unwrap_or_default();

            // Convert core ApiMethodData to Go-specific GoApiMethodData
            let components = openapi.components.as_ref();
            let go_methods: Result<Vec<GoApiMethodData>, GeneratorError> = tag_apis
                .iter()
                .map(|api| {
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
                        body_param: None, // TODO: Extract from request_body
                        has_request_body: api.request_body.is_some(),
                        request_body_content_type: "application/json".to_string(), // TODO: Extract from request_body
                        response_type: None, // TODO: Extract from return_type
                        description: op_info.and_then(|op| op.operation.description.clone()),
                    })
                })
                .collect();

            let go_methods = go_methods?;

            // Create client struct
            let client_struct = GoStruct::new(tag.to_pascal_case());

            // Get SDK name from OpenAPI title
            let sdk_name = openapi.info.title.to_pascal_case();

            // Collect imports
            let imports = vec![
                format!("{}/internal/config", module_path),
                format!("{}/internal/hooks", module_path),
                format!("{}/internal/utils", module_path),
                format!("{}/models/components", module_path),
                format!("{}/models/operations", module_path),
                "context".to_string(),
                "fmt".to_string(),
                "io".to_string(),
                "net/http".to_string(),
                "net/url".to_string(),
            ];

            let api_data =
                ApiOperationData::new(client_struct, tag.clone(), sdk_name, common_header.clone())
                    .with_methods(go_methods)
                    .with_imports(imports);

            // Wrap in context with api_operation key (matching template expectations)
            let template_context = context! {
                api_operation => api_data,
                common_file_header => common_header,
                module_path => module_path.clone(),
            };
            let filename = self.generate_filename(&tag);
            let file_info = self.templates.render_template(
                TemplateName::ApiOperation,
                &filename,
                template_context,
            )?;
            files.push(file_info);
        }

        // Generate main SDK file
        let sdk_name: String = openapi.info.title.to_pascal_case();
        let package_name: String = self
            .config
            .package_name
            .as_ref()
            .map(|s| s.to_snake_case())
            .unwrap_or_else(|| "sdk".to_string());

        let main_sdk_data = MainSdkData::new(sdk_name.clone(), package_name, common_header.clone());
        let template_context = context! {
            main_sdk => main_sdk_data,
            common_file_header => common_header,
        };
        let sdk_filename = if let Some(pkg) = &self.config.package_name {
            format!("{}.go", pkg.to_snake_case())
        } else {
            format!("{}.go", sdk_name.to_snake_case())
        };
        let file_info = self.templates.render_template(
            TemplateName::MainSdk,
            &sdk_filename,
            template_context,
        )?;
        files.push(file_info);

        Ok(files)
    }

    fn generate_models(
        &self,
        openapi: &OpenApi,
        models: Vec<ModelData>,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data.clone());
        let mut files = Vec::new();

        let module_path = self
            .config
            .module_path
            .clone()
            .unwrap_or_else(|| "example.com/sdk".to_string());

        if let Some(components) = &openapi.components {
            for model in models {
                let (type_def, imports, required_fields) = type_mapping::generate_model_data(
                    &model.name,
                    &model.schema,
                    components,
                    &self.config,
                    &header_data,
                )
                .map_err(|e| GeneratorError::ModelGeneration {
                    model_name: model.name.clone(),
                    source: Box::new(e),
                })?;

                // Add module path to imports
                let full_imports: Vec<String> = imports
                    .iter()
                    .map(|imp| {
                        if imp.starts_with("optionalnullable") || imp.starts_with("internal/") {
                            format!("{}/{}", module_path, imp)
                        } else {
                            imp.clone()
                        }
                    })
                    .collect();

                let filename = self.generate_filename(&model.name);
                use crate::ast::ty::GoTypeDefinition;
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
                        let model_data = ModelTypeAliasData::new(t, common_header.clone())
                            .with_imports(full_imports);
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
                files.push(file_info);
            }
        }

        Ok(files)
    }

    fn generate_runtime(
        &self,
        openapi: &OpenApi,
        _: RuntimeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let header_data = HeaderData::from_openapi(openapi);
        let common_header = CommonFileHeaderData::from(header_data);
        let mut files = Vec::new();

        // Generate runtime utility files
        let runtime_files = vec![
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
            ("retry/config.go", TemplateName::RuntimeRetryConfig),
            ("internal/hooks/hooks.go", TemplateName::RuntimeHooks),
            (
                "internal/hooks/registration.go",
                TemplateName::RuntimeHooksRegistration,
            ),
            ("types/pointers.go", TemplateName::TypesPointers),
            ("types/date.go", TemplateName::TypesDate),
            ("types/datetime.go", TemplateName::TypesDateTime),
            ("types/bigint.go", TemplateName::TypesBigInt),
            (
                "optionalnullable/optionalnullable.go",
                TemplateName::TypesOptionalNullable,
            ),
        ];

        let module_path = self
            .config
            .module_path
            .clone()
            .unwrap_or_else(|| "example.com/sdk".to_string());

        for (filename, template_name) in runtime_files {
            let template_context = context! {
                common_file_header => common_header.clone(),
                module_path => module_path.clone(),
            };
            let file_info =
                self.templates
                    .render_template(template_name, filename, template_context)?;
            files.push(file_info);
        }

        Ok(files)
    }

    fn generate_project_files(
        &self,
        _openapi: &OpenApi,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let module_path = self
            .config
            .module_path
            .clone()
            .unwrap_or_else(|| "example.com/sdk".to_string());

        let template_context = context! {
            module_path => module_path,
        };

        let file_info =
            self.templates
                .render_template(TemplateName::GoMod, "go.mod", template_context)?;

        Ok(vec![file_info])
    }

    fn generate_readme(
        &self,
        _openapi: &OpenApi,
        data: ReadmeData,
    ) -> Result<Vec<FileInfo>, Box<dyn Error + Send + Sync>> {
        let default_module = "example.com/sdk".to_string();
        let module_path = self.config.module_path.as_ref().unwrap_or(&default_module);

        let template_context = context! {
            package_name => data.package_name,
            description => data.description,
            version => data.version,
            module_path => module_path,
        };

        let file_info =
            self.templates
                .render_template(TemplateName::Readme, "README.md", template_context)?;

        Ok(vec![file_info])
    }
}

impl FileWriter for GoHttpCodeGenerator {}
