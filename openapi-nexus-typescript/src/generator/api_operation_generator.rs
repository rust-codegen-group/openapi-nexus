//! Individual API operation generator for TypeScript

use std::collections::{BTreeMap, BTreeSet};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use http::Method;
use minijinja::context;
use tracing::error;
use utoipa::openapi;

use crate::ast::{
    TsDocComment, TsExpression, TsInterfaceDefinition, TsInterfaceSignature, TsParameter,
    TsPrimitive, TsProperty,
};
use crate::core::GeneratorError;
use crate::generator::{
    api_interface_builder::ApiInterfaceBuilder, model_import_collector::ModelImportCollector,
    parameter_extractor::ParameterExtractor, response_transformer::ResponseTransformer,
    return_type_generator::ReturnTypeGenerator,
};
use crate::templating::data::{ApiClassData, ApiClassSignature, ApiImportStatement, ApiMethodData};
use crate::templating::data::{
    ApiOperationData, CommonFileHeaderData, HttpParamData, MethodTemplateData,
};
use crate::templating::{TemplateName, Templates};
use crate::utils::schema_mapper::SchemaMapper;
use openapi_nexus_core::data::OperationInfo;
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::OperationInfoExt as _;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Individual API operation generator
#[derive(Debug, Clone)]
pub struct ApiOperationGenerator {
    parameter_extractor: ParameterExtractor,
    response_transformer: ResponseTransformer,
    interface_builder: ApiInterfaceBuilder,
    model_import_collector: ModelImportCollector,
}

impl Default for ApiOperationGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiOperationGenerator {
    /// Create a new API operation generator
    pub fn new() -> Self {
        Self {
            parameter_extractor: ParameterExtractor,
            response_transformer: ResponseTransformer::new(),
            interface_builder: ApiInterfaceBuilder,
            model_import_collector: ModelImportCollector,
        }
    }

    /// Generate an API class for a specific tag with operations
    pub fn generate_api_class(
        &self,
        tag: &str,
        operations: &[OperationInfo],
        templating: &Templates,
        common_file_header: &CommonFileHeaderData,
        components: Option<&openapi::Components>,
    ) -> Result<FileInfo, GeneratorError> {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let interface_name = format!("{}Interface", class_name);

        // Check for duplicate method names and emit errors but continue
        self.check_duplicate_method_names(operations, &class_name);

        let mut methods = vec![
            // Constructor method
            ApiMethodData {
                name: "constructor".to_string(),
                parameters: vec![TsParameter::optional(
                    "configuration".to_string(),
                    TsExpression::Reference("Configuration".to_string()),
                )],
                return_type: None,
                is_async: false,
                documentation: Some(TsDocComment::new("Initialize the API client".to_string())),
            },
        ];

        let mut method_template_data: BTreeMap<String, MethodTemplateData> = BTreeMap::from([(
            "constructor".to_string(),
            MethodTemplateData {
                method_name: "constructor".to_string(),
                body_template: TemplateName::ApiConstructorBaseApi,
                http_params: None,
                convenience_method_name: None,
                convenience_return_type: None,
            },
        )]);

        // Generate methods for each operation and collect request interfaces
        let mut request_interfaces: BTreeMap<String, TsInterfaceDefinition> = BTreeMap::new();
        for op_info in operations {
            // Generate Raw method (returns ApiResponse wrapper)
            let (raw_method, raw_template_data, request_interface) =
                self.generate_operation_method_raw(op_info, components)?;
            method_template_data.insert(raw_method.name.clone(), raw_template_data);
            methods.push(raw_method);

            // Store request interface if present
            if let Some(req_iface) = request_interface {
                let method_base_name = op_info.method_name();
                let interface_name = format!("Api{}Request", method_base_name.to_pascal_case());
                request_interfaces.insert(interface_name, req_iface);
            }
        }

        // Collect model dependencies and build imports
        let dependencies = self.model_import_collector.collect_model_dependencies(
            operations,
            components,
            &self.response_transformer,
        );
        let model_imports = self
            .model_import_collector
            .build_model_imports(&dependencies);

        // Create imports
        let mut imports = vec![
            ApiImportStatement::new("../runtime/runtime".to_string())
                .with_import("BaseAPI".to_string(), None)
                .with_import("JSONApiResponse".to_string(), None)
                .with_import("VoidApiResponse".to_string(), None)
                .with_import("ResponseError".to_string(), None)
                .with_import("RequiredError".to_string(), None)
                .with_import("DefaultConfig".to_string(), None)
                .with_type_import("Configuration".to_string(), None)
                .with_type_import("InitOverrideFunction".to_string(), None),
        ];

        // Add model imports
        imports.extend(model_imports);

        let api_class = ApiClassData {
            is_export: true,
            name: class_name.clone(),
            generics: Vec::new(),
            extends: Some("BaseAPI".to_string()),
            implements: vec![interface_name.clone()],
            signature: ApiClassSignature {
                is_export: true,
                name: class_name.clone(),
                generics: Vec::new(),
                extends: Some("BaseAPI".to_string()),
                implements: vec![interface_name.clone()],
            },
            methods,
            documentation: Some(TsDocComment::new(format!(
                "API client for {} operations",
                tag
            ))),
        };

        // Build interface using the interface builder
        let api_interface = self
            .interface_builder
            .build_interface(&api_class, &method_template_data);

        // Convert request_interfaces BTreeMap to Vec for template iteration
        let request_interfaces_vec: Vec<_> = request_interfaces.into_values().collect();

        let api_operation = ApiOperationData {
            ts_class: api_class.clone(),
            imports,
            ts_interface: api_interface,
            method_templates: method_template_data,
            request_interfaces: request_interfaces_vec,
        };

        let template_data = context! {
            common_file_header,
            api_operation,
        };

        let content = templating
            .render_template_string(TemplateName::ApiOperation, template_data)
            .map_err(|e| GeneratorError::Generic {
                message: format!("Failed to emit API class {}: {}", class_name, e),
            })?;

        // Generate filename based on tag
        let filename = format!("{}Api.ts", tag.to_pascal_case());

        Ok(FileInfo::new(filename, content, FileCategory::Apis))
    }

    /// Generate a Raw method for a specific operation (returns ApiResponse wrapper)
    fn generate_operation_method_raw(
        &self,
        op_info: &OperationInfo,
        components: Option<&openapi::Components>,
    ) -> Result<
        (
            ApiMethodData,
            MethodTemplateData,
            Option<TsInterfaceDefinition>,
        ),
        GeneratorError,
    > {
        let method_name = format!("{}Raw", op_info.method_name());
        let all_parameters = self.generate_method_parameters(&op_info.path, &op_info.operation)?;

        // Generate request interface if there are parameters (excluding initOverrides)
        let request_interface = self.generate_request_interface(op_info, &all_parameters);

        // Use request object if interface exists, otherwise use individual parameters
        let parameters = if request_interface.is_some() {
            let method_base_name = op_info.method_name();
            let interface_name = format!("Api{}Request", method_base_name.to_pascal_case());
            vec![
                TsParameter::new(
                    "requestParameters".to_string(),
                    TsExpression::Reference(interface_name),
                ),
                TsParameter::optional(
                    "initOverrides".to_string(),
                    TsExpression::Union({
                        let mut union = BTreeSet::new();
                        union.insert(TsExpression::Reference("RequestInit".to_string()));
                        union.insert(TsExpression::Reference("InitOverrideFunction".to_string()));
                        union
                    }),
                ),
            ]
        } else {
            all_parameters.clone()
        };

        let (raw_return_type, convenience_return_type) =
            ReturnTypeGenerator::generate_return_types(&op_info.method, &op_info.operation)?;

        // Determine template based on HTTP method
        let template_name = match op_info.method {
            Method::GET => TemplateName::ApiMethodGet,
            Method::POST | Method::PUT | Method::PATCH => TemplateName::ApiMethodPostPutPatch,
            Method::DELETE => TemplateName::ApiMethodDelete,
            _ => {
                return Err(GeneratorError::Generic {
                    message: format!(
                        "Unsupported HTTP method: {:?}. Only GET, POST, PUT, PATCH, and DELETE are supported.",
                        op_info.method
                    ),
                });
            }
        };

        // Create template data
        let template_data = self.create_method_template_data(
            op_info,
            template_name,
            method_name.clone(),
            Some(convenience_return_type),
            request_interface.is_some(),
            components,
        )?;

        // Build enhanced documentation with JSDoc annotations
        // Use original parameters for documentation, not the request object wrapper
        let documentation = self.build_method_documentation(op_info, &all_parameters);

        let method = ApiMethodData {
            name: method_name.clone(),
            parameters,
            return_type: Some(raw_return_type),
            is_async: true,
            documentation,
        };

        Ok((method, template_data, request_interface))
    }

    /// Create template data for method body generation
    fn create_method_template_data(
        &self,
        op_info: &OperationInfo,
        template_name: TemplateName,
        method_name: String,
        convenience_return_type: Option<TsExpression>,
        uses_request_object: bool,
        components: Option<&openapi::Components>,
    ) -> Result<MethodTemplateData, GeneratorError> {
        // Extract parameters using the parameter extractor (names are already resolved)
        let extracted = self.parameter_extractor.extract_parameters(
            &op_info.operation,
            &op_info.path,
            components,
        )?;

        // Parameters are already ParameterInfo, no conversion needed
        let path_params = extracted.path_params;
        let query_params = extracted.query_params;
        let header_params = extracted.header_params;
        let body_param = extracted.body_param;

        let transformer = self
            .response_transformer
            .compute_transformer(&op_info.method, &op_info.operation);

        // Extract body model name if body is an interface (has ToJSON function)
        let body_model_name = self
            .model_import_collector
            .extract_request_body_model_name(&op_info.operation)
            .filter(|model_name| {
                self.model_import_collector
                    .is_schema_interface(model_name, components)
            });

        let http_params = HttpParamData {
            http_path: op_info.path.clone(),
            http_method: op_info.method.clone(),
            path_params,
            query_params,
            header_params,
            body_param,
            body_model_name,
            transformer,
            uses_request_object,
        };

        // Compute convenience method name
        let convenience_method_name = Some(op_info.method_name());

        Ok(MethodTemplateData {
            method_name,
            body_template: template_name,
            http_params: Some(http_params),
            convenience_method_name,
            convenience_return_type,
        })
    }

    /// Generate request parameter interface for a method
    fn generate_request_interface(
        &self,
        op_info: &OperationInfo,
        parameters: &[TsParameter],
    ) -> Option<TsInterfaceDefinition> {
        // Filter out initOverrides parameter
        let actual_params: Vec<&TsParameter> = parameters
            .iter()
            .filter(|p| p.name != "initOverrides")
            .collect();

        // Only create request interface if there are parameters
        if actual_params.is_empty() {
            return None;
        }

        // Create interface name: Api{MethodName}Request
        let method_base_name = op_info.method_name();
        let interface_name = format!("Api{}Request", method_base_name.to_pascal_case());

        // Convert parameters to properties
        let properties: Vec<TsProperty> = actual_params
            .iter()
            .map(|param| {
                let type_expr = param.type_expr.clone();
                // Convert parameter name to camelCase for TypeScript interface
                let camel_case_name = param.name.to_lower_camel_case();
                TsProperty {
                    prop_name: camel_case_name.clone(),
                    original_name: camel_case_name,
                    type_expr,
                    optional: param.optional,
                    documentation: None,
                }
            })
            .collect();

        let signature = TsInterfaceSignature::new(interface_name);
        Some(TsInterfaceDefinition::new(signature).with_properties(properties))
    }

    /// Convert ParameterInfo to TsParameter
    fn parameter_info_to_ts_parameter(
        param_info: &openapi_nexus_core::data::ParameterInfo,
    ) -> TsParameter {
        let type_expr = param_info
            .schema
            .as_ref()
            .map(SchemaMapper::map_ref_or_schema_to_type)
            .unwrap_or(TsExpression::Primitive(TsPrimitive::String));

        TsParameter {
            name: param_info.param_name.clone(),
            type_expr,
            optional: !param_info.required,
            default_value: param_info
                .default_value
                .as_ref()
                .map(ParameterExtractor::value_to_string),
        }
    }

    /// Generate method parameters from operation
    fn generate_method_parameters(
        &self,
        path: &str,
        operation: &openapi::path::Operation,
    ) -> Result<Vec<TsParameter>, GeneratorError> {
        let mut parameters = Vec::new();

        // Extract parameters using the parameter extractor (conflicts are already resolved)
        // Note: components is not available here, so default values from references won't be resolved
        // This is acceptable since this is only used for method signature generation
        let extracted = self
            .parameter_extractor
            .extract_parameters(operation, path, None)?;

        for param_info in extracted.path_params {
            parameters.push(Self::parameter_info_to_ts_parameter(&param_info));
        }
        for param_info in extracted.query_params {
            parameters.push(Self::parameter_info_to_ts_parameter(&param_info));
        }
        for param_info in extracted.header_params {
            parameters.push(Self::parameter_info_to_ts_parameter(&param_info));
        }
        if let Some(body_param) = extracted.body_param {
            parameters.push(Self::parameter_info_to_ts_parameter(&body_param));
        }

        // Add initOverrides parameter at the end
        let mut union: BTreeSet<TsExpression> = BTreeSet::new();
        union.insert(TsExpression::Reference("RequestInit".to_string()));
        union.insert(TsExpression::Reference("InitOverrideFunction".to_string()));
        parameters.push(TsParameter::optional(
            "initOverrides".to_string(),
            TsExpression::Union(union),
        ));

        Ok(parameters)
    }

    /// Build enhanced method documentation with JSDoc annotations
    fn build_method_documentation(
        &self,
        op_info: &OperationInfo,
        parameters: &[TsParameter],
    ) -> Option<TsDocComment> {
        use std::collections::HashMap;

        let mut doc_lines = Vec::new();

        // Start with summary or description
        if let Some(summary) = &op_info.operation.summary {
            doc_lines.push(summary.clone());
        } else if let Some(description) = &op_info.operation.description {
            doc_lines.push(description.clone());
        }

        // Collect parameter information from operation
        let mut param_descriptions: HashMap<String, String> = HashMap::new();

        // Extract from operation parameters
        if let Some(op_params) = &op_info.operation.parameters {
            for param in op_params {
                let desc = param.description.clone().unwrap_or_else(String::new);
                param_descriptions.insert(param.name.clone(), desc);
            }
        }

        // Extract from request body
        if let Some(request_body) = &op_info.operation.request_body
            && let Some(desc) = &request_body.description
        {
            param_descriptions.insert("body".to_string(), desc.clone());
        }

        // Build @param annotations
        let mut jsdoc_params = Vec::new();
        for param in parameters {
            if param.name == "initOverrides" {
                continue; // Skip initOverrides in JSDoc
            }
            let type_str = param.type_expr.to_string_formatted();
            let desc = param_descriptions
                .get(&param.name)
                .cloned()
                .unwrap_or_else(String::new);
            let param_doc = format!("@param {{{}}} {} {}", type_str, param.name, desc)
                .trim_end()
                .to_string();
            if !param_doc.ends_with(&param.name) {
                jsdoc_params.push((param.name.clone(), param_doc));
            } else {
                jsdoc_params.push((
                    param.name.clone(),
                    format!("@param {{{}}} {}", type_str, param.name),
                ));
            }
        }

        // Check for required parameters to add @throws
        let mut throws = Vec::new();
        let has_required_params = parameters
            .iter()
            .any(|p| !p.optional && p.name != "initOverrides");
        if has_required_params {
            throws.push(("RequiredError".to_string(), String::new()));
        }

        // Build complete documentation
        if doc_lines.is_empty() && jsdoc_params.is_empty() && throws.is_empty() {
            return None;
        }

        let mut full_doc = doc_lines.join("\n");
        if !jsdoc_params.is_empty() {
            full_doc.push('\n');
            for (_, param_doc) in &jsdoc_params {
                full_doc.push('\n');
                full_doc.push_str(param_doc);
            }
        }
        if !throws.is_empty() {
            full_doc.push('\n');
            for (error_type, _) in &throws {
                full_doc.push_str(&format!("\n@throws {{{}}}", error_type));
            }
        }

        Some(TsDocComment::new(full_doc.trim().to_string()))
    }

    /// Check for duplicate method names in operations and emit errors
    ///
    /// This method checks for duplicate Raw method names and convenience method names.
    /// If duplicates are found, error messages are logged but generation continues,
    /// allowing TypeScript compilation to fail with the duplicate method errors.
    fn check_duplicate_method_names(&self, operations: &[OperationInfo], class_name: &str) {
        let mut used_raw_names: BTreeSet<String> = BTreeSet::new();
        let mut used_convenience_names: BTreeSet<String> = BTreeSet::new();

        for op_info in operations {
            let base_method_name = op_info.method_name();
            let raw_method_name = format!("{}Raw", base_method_name);

            // Check for duplicate Raw method names
            if used_raw_names.contains(&raw_method_name) {
                error!(
                    "Duplicate method name detected: '{}' for operation {} {} in API class '{}'. This will cause TypeScript compilation errors.",
                    raw_method_name, op_info.method, op_info.path, class_name
                );
            }
            used_raw_names.insert(raw_method_name);

            // Check for duplicate convenience method names
            if used_convenience_names.contains(&base_method_name) {
                error!(
                    "Duplicate convenience method name detected: '{}' for operation {} {} in API class '{}'. This will cause TypeScript compilation errors.",
                    base_method_name, op_info.method, op_info.path, class_name
                );
            }
            used_convenience_names.insert(base_method_name);
        }
    }
}
