//! Individual API operation generator for TypeScript

use std::collections::{BTreeMap, BTreeSet};

use heck::ToPascalCase as _;
use http::Method;
use minijinja::context;
use utoipa::openapi;

use crate::ast::{
    TsClassDefinition, TsClassMethod, TsDocComment, TsExpression, TsImportStatement, TsParameter,
};
use crate::core::GeneratorError;
use crate::emission::error::EmitError;
use crate::generator::{
    api_interface_builder::ApiInterfaceBuilder, parameter_extractor::ParameterExtractor,
    response_transformer::ResponseTransformer, return_type_generator::ReturnTypeGenerator,
};
use crate::templating::data::{
    ApiOperationData, CommonFileHeaderData, HttpParamData, MethodTemplateData,
};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_core::data::{OperationInfo, ParameterInfo as CoreParameterInfo};
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::OperationInfoExt as _;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Individual API operation generator
#[derive(Debug, Clone)]
pub struct ApiOperationGenerator {
    parameter_extractor: ParameterExtractor,
    return_type_generator: ReturnTypeGenerator,
    response_transformer: ResponseTransformer,
    interface_builder: ApiInterfaceBuilder,
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
            parameter_extractor: ParameterExtractor::new(),
            return_type_generator: ReturnTypeGenerator::new(),
            response_transformer: ResponseTransformer::new(),
            interface_builder: ApiInterfaceBuilder::new(),
        }
    }

    /// Generate an API class for a specific tag with operations
    pub fn generate_api_class(
        &self,
        tag: &str,
        operations: &[OperationInfo],
        templating: &Templates,
        common_file_header: &CommonFileHeaderData,
    ) -> Result<FileInfo, GeneratorError> {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let interface_name = format!("{}Interface", class_name);

        let mut methods = vec![
            // Constructor method
            TsClassMethod::new("constructor".to_string())
                .with_parameters(vec![TsParameter::optional(
                    "configuration".to_string(),
                    Some(TsExpression::Reference("Configuration".to_string())),
                )])
                .with_docs(TsDocComment::new("Initialize the API client".to_string())),
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

        // Generate methods for each operation
        for op_info in operations {
            // Generate Raw method (returns ApiResponse wrapper)
            let (raw_method, raw_template_data) = self.generate_operation_method_raw(op_info)?;
            method_template_data.insert(raw_method.name.clone(), raw_template_data);
            methods.push(raw_method);
        }

        // Collect model imports for FromJSON transformers
        let mut model_imports: BTreeSet<String> = BTreeSet::new();
        for op_info in operations {
            if let Some((_, model_name)) = self
                .response_transformer
                .compute_transformer_and_model(&op_info.method, &op_info.operation)
            {
                model_imports.insert(model_name);
            }
        }

        // Create imports
        let mut imports = vec![
            TsImportStatement::new("../runtime/runtime".to_string())
                .with_import("BaseAPI".to_string(), None)
                .with_import("JSONApiResponse".to_string(), None)
                .with_import("VoidApiResponse".to_string(), None)
                .with_import("ResponseError".to_string(), None)
                .with_type_import("Configuration".to_string(), None)
                .with_type_import("InitOverrideFunction".to_string(), None),
        ];

        // Add model helper imports
        for name in model_imports {
            imports.push(
                TsImportStatement::new(format!("../models/{}", name))
                    .with_import(format!("{}FromJSON", name), None),
            );
        }

        let api_class = TsClassDefinition::new(class_name.clone())
            .with_methods(methods)
            .with_extends("BaseAPI".to_string())
            .with_implements(vec![interface_name.clone()])
            .with_docs(TsDocComment::new(format!(
                "API client for {} operations",
                tag
            )))
            .with_imports(imports);

        // Render the class to code
        let content = self
            .emit_class(
                templating,
                &api_class,
                method_template_data,
                common_file_header,
            )
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
    ) -> Result<(TsClassMethod, MethodTemplateData), GeneratorError> {
        let method_name = format!("{}Raw", op_info.method_name());
        let parameters = self.generate_method_parameters(&op_info.path, &op_info.operation)?;

        let (raw_return_type, convenience_return_type) = self
            .return_type_generator
            .generate_return_types(&op_info.method, &op_info.operation)?;

        // Determine template based on HTTP method
        let template_name = match op_info.method {
            Method::GET => TemplateName::ApiMethodGet,
            Method::POST | Method::PUT | Method::PATCH => TemplateName::ApiMethodPostPutPatch,
            Method::DELETE => TemplateName::ApiMethodDelete,
            _ => TemplateName::ApiDefaultMethod,
        };

        // Create template data
        let template_data = self.create_method_template_data(
            op_info,
            template_name,
            method_name.clone(),
            convenience_return_type,
        )?;

        let mut method = TsClassMethod::new(method_name.clone())
            .with_parameters(parameters)
            .with_async();

        if let Some(return_type) = raw_return_type {
            method = method.with_return_type(return_type);
        }

        if let Some(docs) = op_info
            .operation
            .summary
            .clone()
            .or_else(|| op_info.operation.description.clone())
        {
            method = method.with_docs(TsDocComment::new(docs));
        }

        Ok((method, template_data))
    }

    /// Create template data for method body generation
    fn create_method_template_data(
        &self,
        op_info: &OperationInfo,
        template_name: TemplateName,
        method_name: String,
        convenience_return_type: Option<TsExpression>,
    ) -> Result<MethodTemplateData, GeneratorError> {
        // Extract parameters from operation for template data
        let mut path_params_core = Vec::new();
        let mut query_params_core = Vec::new();
        let mut header_params_core = Vec::new();

        if let Some(params) = &op_info.operation.parameters {
            for param in params {
                let param_info = CoreParameterInfo {
                    name: param.name.clone(),
                    schema: param.schema.clone(),
                    required: matches!(param.required, openapi::Required::True),
                    deprecated: matches!(param.deprecated, Some(openapi::Deprecated::True)),
                    location: param.parameter_in.clone(),
                };

                match param.parameter_in {
                    openapi::path::ParameterIn::Path => path_params_core.push(param_info),
                    openapi::path::ParameterIn::Query => query_params_core.push(param_info),
                    openapi::path::ParameterIn::Header => header_params_core.push(param_info),
                    openapi::path::ParameterIn::Cookie => header_params_core.push(param_info),
                }
            }
        }

        // Construct body_param from request_body
        // Note: body is not a ParameterIn location, so we use Path as a placeholder
        let body_param = op_info.operation.request_body.as_ref().map(|rb| {
            CoreParameterInfo {
                name: "body".to_string(),
                schema: None, // Could extract from RequestBody.content if needed
                required: matches!(rb.required, Some(openapi::Required::True)),
                deprecated: false,
                location: openapi::path::ParameterIn::Path, // Placeholder, not used for body
            }
        });

        let transformer = self
            .response_transformer
            .compute_transformer_and_model(&op_info.method, &op_info.operation)
            .map(|(expr, _)| expr);

        let http_params = HttpParamData {
            http_path: op_info.path.clone(),
            http_method: op_info.method.clone(),
            path_params: path_params_core,
            query_params: query_params_core,
            header_params: header_params_core,
            body_param,
            transformer,
        };

        // Compute convenience method name
        let convenience_method_name = Some(
            method_name
                .strip_suffix("Raw")
                .unwrap_or(&method_name)
                .to_string(),
        );

        Ok(MethodTemplateData {
            method_name,
            body_template: template_name,
            http_params: Some(http_params),
            convenience_method_name,
            convenience_return_type,
        })
    }

    /// Generate method parameters from operation
    fn generate_method_parameters(
        &self,
        path: &str,
        operation: &openapi::path::Operation,
    ) -> Result<Vec<TsParameter>, GeneratorError> {
        let mut parameters = Vec::new();

        // Extract parameters using the parameter extractor
        let extracted = self
            .parameter_extractor
            .extract_parameters(operation, path)?;

        // Add path parameters
        for param_info in extracted.path_params {
            parameters.push(TsParameter {
                name: param_info.name,
                type_expr: Some(param_info.type_expr),
                optional: !param_info.required,
                default_value: param_info.default_value,
            });
        }

        // Add query parameters
        for param_info in extracted.query_params {
            parameters.push(TsParameter {
                name: param_info.name,
                type_expr: Some(param_info.type_expr),
                optional: !param_info.required,
                default_value: param_info.default_value,
            });
        }

        // Add header parameters
        for param_info in extracted.header_params {
            parameters.push(TsParameter {
                name: param_info.name,
                type_expr: Some(param_info.type_expr),
                optional: !param_info.required,
                default_value: param_info.default_value,
            });
        }

        // Add request body parameter
        if let Some(body_param) = extracted.body_param {
            parameters.push(TsParameter {
                name: body_param.name,
                type_expr: Some(body_param.type_expr),
                optional: !body_param.required,
                default_value: body_param.default_value,
            });
        }

        // Add initOverrides parameter at the end
        let mut union: BTreeSet<TsExpression> = BTreeSet::new();
        union.insert(TsExpression::Reference("RequestInit".to_string()));
        union.insert(TsExpression::Reference("InitOverrideFunction".to_string()));
        parameters.push(TsParameter::optional(
            "initOverrides".to_string(),
            Some(TsExpression::Union(union)),
        ));

        Ok(parameters)
    }

    /// Emit TypeScript code from a class definition
    fn emit_class(
        &self,
        templating: &Templates,
        class: &TsClassDefinition,
        method_template_data: BTreeMap<String, MethodTemplateData>,
        common_file_header: &CommonFileHeaderData,
    ) -> Result<String, EmitError> {
        let class = class.clone();

        // Build interface using the interface builder
        let api_interface = self
            .interface_builder
            .build_interface(&class, &method_template_data);

        let imports = class.imports.clone();
        let api_operation =
            ApiOperationData::new(class.clone(), imports, api_interface, method_template_data);

        let template_data = context! {
            common_file_header,
            api_operation,
        };

        // Get the API class template and render directly
        templating.render_template_string(TemplateName::ApiOperation, template_data)
    }
}
