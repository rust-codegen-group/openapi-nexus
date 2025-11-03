//! Individual API class generator for TypeScript

use std::collections::BTreeSet;

use heck::ToPascalCase as _;
use http::Method;
use utoipa::openapi::RefOr;
use utoipa::openapi::path::Operation;
use utoipa::openapi::schema::{ArrayItems, Schema};

use crate::ast::{
    TsClassDefinition, TsClassMethod, TsDocComment, TsExpression, TsImportStatement, TsParameter,
};
use crate::core::GeneratorError;
use crate::generator::parameter_extractor::ParameterExtractor;
use crate::templating::data::ApiMethodBodyData;
use crate::templating::{TemplateName, Templates};
use crate::utils::schema_mapper::SchemaMapper;
use openapi_nexus_core::data::{
    ApiMethodData as CoreApiMethodData, OperationInfo, ParameterInfo as CoreParameterInfo,
};
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::OperationInfoExt as _;
use openapi_nexus_core::traits::file_writer::FileInfo;

/// Individual API class generator
#[derive(Debug, Clone)]
pub struct ApiClassGenerator {
    parameter_extractor: ParameterExtractor,
    schema_mapper: SchemaMapper,
}

impl Default for ApiClassGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiClassGenerator {
    /// Create a new API class generator
    pub fn new() -> Self {
        Self {
            parameter_extractor: ParameterExtractor::new(),
            schema_mapper: SchemaMapper::new(),
        }
    }

    /// Generate an API class for a specific tag with operations
    pub fn generate_api_class(
        &self,
        tag: &str,
        operations: &[OperationInfo],
        templating: &Templates,
        title: Option<&str>,
        description: Option<&str>,
        version: Option<&str>,
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
                .with_docs(TsDocComment::new("Initialize the API client".to_string()))
                .with_body_template(
                    std::path::Path::new(TemplateName::ApiConstructorBaseApi.file_path())
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("constructor_base_api")
                        .to_string(),
                    None,
                ),
        ];

        // Generate methods for each operation
        for op_info in operations {
            // Generate Raw method (returns ApiResponse wrapper)
            let raw_method = self.generate_operation_method_raw(op_info)?;
            methods.push(raw_method.clone());

            // Generate convenience method (unwraps value from Raw)
            let convenience_method = self.generate_operation_method_convenience(op_info)?;
            methods.push(convenience_method);
        }

        // Collect model imports for FromJSON transformers
        let mut model_imports: BTreeSet<String> = BTreeSet::new();
        for op_info in operations {
            if let Some((_, model_name)) =
                self.compute_transformer_and_model(&op_info.method, &op_info.operation)
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
        let content = templating
            .emit_class(&api_class, title, description, version)
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
    ) -> Result<TsClassMethod, GeneratorError> {
        let method_name = format!("{}Raw", op_info.method_name());
        let parameters = self.generate_method_parameters(&op_info.path, &op_info.operation)?;
        let return_type = self.generate_raw_return_type(&op_info.method, &op_info.operation)?;

        // Determine template based on HTTP method
        let template_name = match op_info.method {
            Method::GET => TemplateName::ApiMethodGet,
            Method::POST | Method::PUT | Method::PATCH => TemplateName::ApiMethodPostPutPatch,
            Method::DELETE => TemplateName::ApiMethodDelete,
            _ => TemplateName::ApiDefaultMethod,
        };

        // Create template data
        let template_data = self.create_method_template_data(op_info)?;

        let template_path = template_name.file_path();
        let template_filename = std::path::Path::new(template_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        let mut method = TsClassMethod::new(method_name)
            .with_parameters(parameters)
            .with_async()
            .with_body_template(template_filename.to_string(), Some(template_data));

        if let Some(return_type) = return_type {
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

        Ok(method)
    }

    /// Generate a convenience method that calls the Raw method and unwraps the value
    fn generate_operation_method_convenience(
        &self,
        op_info: &OperationInfo,
    ) -> Result<TsClassMethod, GeneratorError> {
        let base_name = self.generate_method_name_from_op_info(op_info);
        let parameters = self.generate_method_parameters(&op_info.path, &op_info.operation)?;

        let template_filename =
            std::path::Path::new(TemplateName::ApiMethodConvenience.file_path())
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("api_method_convenience");
        let mut method = TsClassMethod::new(base_name)
            .with_parameters(parameters)
            .with_async()
            .with_body_template(template_filename.to_string(), None);

        let convenience_return = self
            .generate_convenience_return_type(&op_info.method, &op_info.operation)?
            .unwrap_or_else(|| TsExpression::Reference("Promise<any>".to_string()));
        method = method.with_return_type(convenience_return);

        if let Some(docs) = op_info
            .operation
            .summary
            .clone()
            .or_else(|| op_info.operation.description.clone())
        {
            method = method.with_docs(TsDocComment::new(docs));
        }

        Ok(method)
    }

    /// Create template data for method body generation
    fn create_method_template_data(
        &self,
        op_info: &OperationInfo,
    ) -> Result<ApiMethodBodyData, GeneratorError> {
        // Extract parameters from operation to build core ApiMethodData
        let mut path_params_core = Vec::new();
        let mut query_params_core = Vec::new();
        let mut header_params_core = Vec::new();

        if let Some(params) = &op_info.operation.parameters {
            for param in params {
                let param_info = CoreParameterInfo {
                    name: param.name.clone(),
                    schema: param.schema.clone(),
                    required: matches!(param.required, utoipa::openapi::Required::True),
                    deprecated: matches!(param.deprecated, Some(utoipa::openapi::Deprecated::True)),
                    location: param.parameter_in.clone(),
                };

                match param.parameter_in {
                    utoipa::openapi::path::ParameterIn::Path => path_params_core.push(param_info),
                    utoipa::openapi::path::ParameterIn::Query => query_params_core.push(param_info),
                    utoipa::openapi::path::ParameterIn::Header => {
                        header_params_core.push(param_info)
                    }
                    utoipa::openapi::path::ParameterIn::Cookie => {
                        header_params_core.push(param_info)
                    }
                }
            }
        }

        let core_method_data = CoreApiMethodData {
            method_name: self.generate_method_name_from_op_info(op_info),
            http_method: op_info.method.clone(),
            path: op_info.path.clone(),
            path_params: path_params_core,
            query_params: query_params_core,
            header_params: header_params_core,
            request_body: op_info.operation.request_body.clone(),
            return_type: None,
            has_auth: op_info.operation.security.is_some(),
            has_error_handling: true,
        };

        let transformer = self
            .compute_transformer_and_model(&op_info.method, &op_info.operation)
            .map(|(expr, _)| expr);

        // Convert to ApiMethodBodyData
        Ok(ApiMethodBodyData::from_core(&core_method_data, transformer))
    }

    /// Generate method name from operation info
    fn generate_method_name_from_op_info(&self, op_info: &OperationInfo) -> String {
        // Use the OperationInfoExt trait method which handles this logic
        op_info.method_name()
    }

    /// Generate method parameters from operation
    fn generate_method_parameters(
        &self,
        path: &str,
        operation: &Operation,
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

    /// Determine Raw return type (ApiResponse wrappers) based on operation responses
    fn generate_raw_return_type(
        &self,
        http_method: &Method,
        operation: &Operation,
    ) -> Result<Option<TsExpression>, GeneratorError> {
        // Look for successful response (200, 201, etc.)
        for (status_code, response_ref) in operation.responses.responses.iter() {
            if status_code.starts_with('2') {
                match response_ref {
                    RefOr::T(response) => {
                        if let Some(json_content) = response.content.get("application/json")
                            && let Some(schema_ref) = &json_content.schema
                        {
                            let return_type =
                                self.schema_mapper.map_ref_or_schema_to_type(schema_ref);
                            return Ok(Some(TsExpression::Reference(format!(
                                "Promise<JSONApiResponse<{}>>",
                                return_type
                            ))));
                        }
                        // No JSON content: treat as void
                        return Ok(Some(TsExpression::Reference(
                            "Promise<VoidApiResponse>".to_string(),
                        )));
                    }
                    RefOr::Ref(_) => {
                        // TODO: Handle response references
                    }
                }
            }
        }

        // Fallbacks: DELETE with no content -> VoidApiResponse; otherwise JSON any
        if *http_method == Method::DELETE {
            return Ok(Some(TsExpression::Reference(
                "Promise<VoidApiResponse>".to_string(),
            )));
        }
        Ok(Some(TsExpression::Reference(
            "Promise<JSONApiResponse<any>>".to_string(),
        )))
    }

    /// Determine convenience return type (unwrapped)
    fn generate_convenience_return_type(
        &self,
        http_method: &Method,
        operation: &Operation,
    ) -> Result<Option<TsExpression>, GeneratorError> {
        // Look for JSON success schema
        for (status_code, response_ref) in operation.responses.responses.iter() {
            if status_code.starts_with('2') {
                match response_ref {
                    RefOr::T(response) => {
                        if let Some(json_content) = response.content.get("application/json")
                            && let Some(schema_ref) = &json_content.schema
                        {
                            let t = self.schema_mapper.map_ref_or_schema_to_type(schema_ref);
                            return Ok(Some(TsExpression::Reference(format!("Promise<{}>", t))));
                        }
                        return Ok(Some(TsExpression::Reference("Promise<void>".to_string())));
                    }
                    RefOr::Ref(_) => {}
                }
            }
        }
        if *http_method == Method::DELETE {
            return Ok(Some(TsExpression::Reference("Promise<void>".to_string())));
        }
        Ok(Some(TsExpression::Reference("Promise<any>".to_string())))
    }

    /// Compute JSON transformer expression and model name if applicable
    fn compute_transformer_and_model(
        &self,
        _http_method: &Method,
        operation: &Operation,
    ) -> Option<(String, String)> {
        for (status_code, response_ref) in operation.responses.responses.iter() {
            if !status_code.starts_with('2') {
                continue;
            }
            if let RefOr::T(response) = response_ref
                && let Some(json_content) = response.content.get("application/json")
                && let Some(schema_ref) = &json_content.schema
            {
                match schema_ref {
                    RefOr::Ref(reference) => {
                        if let Some(name) =
                            reference.ref_location.strip_prefix("#/components/schemas/")
                        {
                            let expr = format!("(jsonValue) => {}FromJSON(jsonValue)", name);
                            return Some((expr, name.to_string()));
                        }
                    }
                    RefOr::T(schema) => {
                        if let Schema::Array(arr) = schema {
                            match &arr.items {
                                ArrayItems::RefOrSchema(item_ref) => {
                                    if let RefOr::Ref(reference) = &**item_ref
                                        && let Some(name) = reference
                                            .ref_location
                                            .strip_prefix("#/components/schemas/")
                                    {
                                        let expr = format!(
                                            "(jsonValue) => (jsonValue as Array<any>).map({}FromJSON)",
                                            name
                                        );
                                        return Some((expr, name.to_string()));
                                    }
                                }
                                ArrayItems::False => {}
                            }
                        }
                    }
                }
            }
        }
        None
    }
}
