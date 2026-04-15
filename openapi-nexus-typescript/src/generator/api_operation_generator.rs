//! Individual API operation generator for TypeScript

use std::collections::{BTreeMap, BTreeSet, HashMap};

use heck::{ToLowerCamelCase as _, ToPascalCase as _};
use http::Method;
use indexmap::IndexMap;
use minijinja::context;
use tracing::error;

use crate::ast::{
    TsDocComment, TsExpression, TsInterfaceDefinition, TsInterfaceSignature, TsParameter,
    TsPrimitive, TsProperty,
};
use crate::errors::GeneratorError;
use crate::generator::{
    api_interface_builder::ApiInterfaceBuilder, ir_schema_generator::IrSchemaGenerator,
};
use crate::templating::data::{ApiClassData, ApiClassSignature, ApiImportStatement, ApiMethodData};
use crate::templating::data::{
    ApiOperationData, CommonFileHeaderData, HttpParamData, MethodTemplateData, ResponseTemplateData,
};
use crate::templating::{TemplateName, Templates};
use openapi_nexus_core::data::ParameterInfo;
use openapi_nexus_core::data::StatusCode;
use openapi_nexus_core::data::{self};
use openapi_nexus_core::traits::FileCategory;
use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::{
    IrOperation, IrResponse, IrSchema, IrSchemaKind, IrTypeExpr,
    ParameterLocation as IrParameterLocation,
};

/// Helper struct to hold all response template data
struct ResponseTemplates {
    success_responses: BTreeMap<String, ResponseTemplateData>,
    error_responses: BTreeMap<String, ResponseTemplateData>,
    default_response: Option<ResponseTemplateData>,
    fallback_response: Option<ResponseTemplateData>,
}

/// Individual API operation generator
#[derive(Debug, Clone)]
pub struct ApiOperationGenerator {
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
            interface_builder: ApiInterfaceBuilder,
        }
    }

    // =========================================================================
    // IR-based API generation
    // =========================================================================

    /// Generate an API class from IR operations (replaces `generate_api_class`)
    pub fn generate_api_class_from_ir(
        &self,
        tag: &str,
        operations: &[&IrOperation],
        schemas: &IndexMap<String, IrSchema>,
        templating: &Templates,
        common_file_header: &CommonFileHeaderData,
    ) -> Result<FileInfo, GeneratorError> {
        let class_name = format!("{}Api", tag.to_pascal_case());
        let interface_name = format!("{}Interface", class_name);

        // Check for duplicate method names
        self.check_duplicate_ir_method_names(operations, &class_name);

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
        let mut model_dependencies = IrModelDependencies::new();

        for ir_op in operations {
            let (raw_method, raw_template_data, request_interface) =
                self.generate_ir_operation_method_raw(ir_op, schemas, &mut model_dependencies)?;
            method_template_data.insert(raw_method.name.clone(), raw_template_data);
            methods.push(raw_method);

            if let Some(req_iface) = request_interface {
                let method_base_name = ir_op.operation_id.to_lower_camel_case();
                let interface_name = format!("Api{}Request", method_base_name.to_pascal_case());
                request_interfaces.insert(interface_name, req_iface);
            }
        }

        // Build model imports from collected dependencies
        let model_imports = Self::build_ir_model_imports(&model_dependencies);

        // Create imports
        let mut imports = vec![
            ApiImportStatement::new("../runtime/runtime".to_string())
                .with_import("BaseAPI".to_string(), None)
                .with_import("JSONApiResponse".to_string(), None)
                .with_import("VoidApiResponse".to_string(), None)
                .with_import("BlobApiResponse".to_string(), None)
                .with_import("TextApiResponse".to_string(), None)
                .with_import("ResponseError".to_string(), None)
                .with_import("RequiredError".to_string(), None)
                .with_import("DefaultConfig".to_string(), None)
                .with_type_import("Configuration".to_string(), None)
                .with_type_import("InitOverrideFunction".to_string(), None),
        ];

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

        let api_interface = self
            .interface_builder
            .build_interface(&api_class, &method_template_data);

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
            .map_err(|e| GeneratorError::ApiClassGeneration {
                class_name: class_name.clone(),
                source: Box::new(e),
            })?;

        let filename = format!("{}Api.ts", tag.to_pascal_case());

        Ok(FileInfo::new(filename, content, FileCategory::Apis))
    }

    /// Generate a Raw method for an IR operation
    fn generate_ir_operation_method_raw(
        &self,
        ir_op: &IrOperation,
        schemas: &IndexMap<String, IrSchema>,
        model_deps: &mut IrModelDependencies,
    ) -> Result<
        (
            ApiMethodData,
            MethodTemplateData,
            Option<TsInterfaceDefinition>,
        ),
        GeneratorError,
    > {
        let method_base_name = ir_op.operation_id.to_lower_camel_case();
        let method_name = format!("{}Raw", method_base_name);

        // Extract parameters from IR
        let extracted = Self::extract_ir_parameters(ir_op);
        let all_parameters = Self::ir_extracted_to_ts_parameters(&extracted);

        // Generate request interface
        let request_interface =
            Self::generate_ir_request_interface(&method_base_name, &all_parameters);

        // Use request object if interface exists
        let parameters = if request_interface.is_some() {
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

        // Classify responses
        let (success_responses, error_responses, default_response) =
            Self::classify_ir_responses(&ir_op.responses);

        // Collect model dependencies from this operation
        Self::collect_ir_operation_dependencies(ir_op, schemas, model_deps);

        // Build return type info
        let any_response_has_body =
            Self::ir_any_response_has_body(&success_responses, &error_responses, &default_response);

        let raw_return_type = Self::generate_ir_raw_return_type(
            &success_responses,
            &error_responses,
            &default_response,
            any_response_has_body,
        );

        let convenience_return_type = Self::generate_ir_convenience_return_type(
            &success_responses,
            &error_responses,
            &default_response,
            any_response_has_body,
        );

        // Build HTTP params
        let uses_request_object = !extracted.path_params.is_empty()
            || !extracted.query_params.is_empty()
            || !extracted.header_params.is_empty()
            || extracted.body_param.is_some();

        let body_model_name = Self::extract_ir_body_model_name(ir_op, schemas);

        let response_templates = Self::build_ir_response_templates(
            &success_responses,
            &error_responses,
            &default_response,
        );

        let http_method: Method = ir_op.method.to_uppercase().parse().map_err(|_| {
            GeneratorError::UnsupportedHttpMethod {
                method: Method::from_bytes(ir_op.method.as_bytes()).unwrap_or(Method::GET),
            }
        })?;

        let template_name = Self::determine_template_name(&http_method)?;

        let http_params = HttpParamData {
            http_path: ir_op.path.clone(),
            http_method: http_method.clone(),
            path_params: extracted.path_params,
            query_params: extracted.query_params,
            header_params: extracted.header_params,
            body_param: extracted.body_param,
            body_model_name,
            transformer: None,
            uses_request_object,
            success_responses: response_templates.success_responses,
            error_responses: response_templates.error_responses,
            default_response: response_templates.default_response,
            fallback_response: response_templates.fallback_response,
        };

        let template_data = MethodTemplateData {
            method_name: method_name.clone(),
            body_template: template_name,
            http_params: Some(http_params),
            convenience_method_name: Some(method_base_name.clone()),
            convenience_return_type: Some(convenience_return_type),
        };

        let documentation = Self::build_ir_method_documentation(ir_op, &all_parameters);

        let method = ApiMethodData {
            name: method_name,
            parameters,
            return_type: Some(raw_return_type),
            is_async: true,
            documentation,
        };

        Ok((method, template_data, request_interface))
    }

    /// Extract parameters from an IR operation into ParameterInfo structs
    fn extract_ir_parameters(ir_op: &IrOperation) -> IrExtractedParameters {
        let mut path_params = Vec::new();
        let mut query_params = Vec::new();
        let mut header_params = Vec::new();

        for ir_param in &ir_op.parameters {
            let location = match ir_param.location {
                IrParameterLocation::Path => data::ParameterLocation::Path,
                IrParameterLocation::Query => data::ParameterLocation::Query,
                IrParameterLocation::Header => data::ParameterLocation::Header,
                IrParameterLocation::Cookie => continue, // Cookie params not supported in fetch
            };

            let param_info = ParameterInfo {
                original_name: ir_param.name.clone(),
                param_name: ir_param.name.clone(), // Will be resolved below
                schema: None,                      // IR params don't carry raw schemas
                required: ir_param.required,
                deprecated: false,
                description: ir_param.description.clone(),
                default_value: ir_param.default_value.clone(),
                location,
            };

            match location {
                data::ParameterLocation::Path => path_params.push(param_info),
                data::ParameterLocation::Query => query_params.push(param_info),
                data::ParameterLocation::Header => header_params.push(param_info),
                data::ParameterLocation::Body => {
                    unreachable!("Body parameters should not reach here")
                }
            }
        }

        // Extract request body
        let body_param = ir_op.request_body.as_ref().and_then(|rb| {
            // Prefer application/json content
            let type_expr = rb
                .content
                .get("application/json")
                .or_else(|| rb.content.values().next());
            type_expr.map(|_| ParameterInfo {
                original_name: "body".to_string(),
                param_name: "body".to_string(),
                schema: None,
                required: rb.required,
                deprecated: false,
                description: rb.description.clone(),
                default_value: None,
                location: data::ParameterLocation::Body,
            })
        });

        let mut extracted = IrExtractedParameters {
            path_params,
            query_params,
            header_params,
            body_param,
            ir_param_types: HashMap::new(),
        };

        // Store IR type expressions for TsParameter generation
        for ir_param in &ir_op.parameters {
            extracted
                .ir_param_types
                .insert(ir_param.name.clone(), ir_param.type_expr.clone());
        }
        if let Some(rb) = &ir_op.request_body
            && let Some(type_expr) = rb
                .content
                .get("application/json")
                .or_else(|| rb.content.values().next())
        {
            extracted
                .ir_param_types
                .insert("body".to_string(), type_expr.clone());
        }

        // Resolve name conflicts (same logic as ParameterExtractor)
        Self::resolve_ir_name_conflicts(&mut extracted);

        extracted
    }

    /// Resolve parameter name conflicts for IR-extracted parameters
    fn resolve_ir_name_conflicts(extracted: &mut IrExtractedParameters) {
        let mut all_params: Vec<(&str, data::ParameterLocation)> = Vec::new();

        for param in &extracted.path_params {
            all_params.push((&param.original_name, param.location));
        }
        for param in &extracted.query_params {
            all_params.push((&param.original_name, param.location));
        }
        for param in &extracted.header_params {
            all_params.push((&param.original_name, param.location));
        }
        if let Some(body_param) = &extracted.body_param {
            all_params.push((&body_param.original_name, body_param.location));
        }

        let mut camel_case_groups: HashMap<String, Vec<(String, data::ParameterLocation)>> =
            HashMap::new();
        for (original_name, location) in all_params {
            let camel_case = original_name.to_lower_camel_case();
            camel_case_groups
                .entry(camel_case)
                .or_default()
                .push((original_name.to_string(), location));
        }

        let mut name_map: HashMap<(String, data::ParameterLocation), String> = HashMap::new();

        for params in camel_case_groups.values() {
            if params.len() > 1 {
                for (original_name, location) in params {
                    let prefix = match location {
                        data::ParameterLocation::Body => "body",
                        data::ParameterLocation::Path => "path",
                        data::ParameterLocation::Query => "query",
                        data::ParameterLocation::Header => "header",
                    };
                    let resolved_name = format!("{}{}", prefix, original_name.to_pascal_case())
                        .to_lower_camel_case();
                    name_map.insert((original_name.clone(), *location), resolved_name);
                }
            } else {
                let (original_name, location) = &params[0];
                let resolved_name = original_name.to_lower_camel_case();
                name_map.insert((original_name.clone(), *location), resolved_name);
            }
        }

        for param in &mut extracted.path_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        for param in &mut extracted.query_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        for param in &mut extracted.header_params {
            if let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location)) {
                param.param_name = resolved.clone();
            }
        }
        if let Some(param) = &mut extracted.body_param
            && let Some(resolved) = name_map.get(&(param.original_name.clone(), param.location))
        {
            param.param_name = resolved.clone();
        }
    }

    /// Convert IR extracted parameters to TsParameter list
    fn ir_extracted_to_ts_parameters(extracted: &IrExtractedParameters) -> Vec<TsParameter> {
        let mut parameters = Vec::new();

        let make_ts_param = |info: &ParameterInfo, ir_types: &HashMap<String, IrTypeExpr>| {
            let type_expr = ir_types
                .get(&info.original_name)
                .map(IrSchemaGenerator::type_expr_to_ts)
                .unwrap_or(TsExpression::Primitive(TsPrimitive::String));

            TsParameter {
                name: info.param_name.clone(),
                type_expr,
                optional: !info.required,
                default_value: info.default_value.as_ref().map(value_to_string),
            }
        };

        for param_info in &extracted.path_params {
            parameters.push(make_ts_param(param_info, &extracted.ir_param_types));
        }
        for param_info in &extracted.query_params {
            parameters.push(make_ts_param(param_info, &extracted.ir_param_types));
        }
        for param_info in &extracted.header_params {
            parameters.push(make_ts_param(param_info, &extracted.ir_param_types));
        }
        if let Some(body_param) = &extracted.body_param {
            parameters.push(make_ts_param(body_param, &extracted.ir_param_types));
        }

        // Add initOverrides parameter at the end
        let mut union: BTreeSet<TsExpression> = BTreeSet::new();
        union.insert(TsExpression::Reference("RequestInit".to_string()));
        union.insert(TsExpression::Reference("InitOverrideFunction".to_string()));
        parameters.push(TsParameter::optional(
            "initOverrides".to_string(),
            TsExpression::Union(union),
        ));

        parameters
    }

    /// Generate request interface from IR parameters
    fn generate_ir_request_interface(
        method_base_name: &str,
        parameters: &[TsParameter],
    ) -> Option<TsInterfaceDefinition> {
        let actual_params: Vec<&TsParameter> = parameters
            .iter()
            .filter(|p| p.name != "initOverrides")
            .collect();

        if actual_params.is_empty() {
            return None;
        }

        let interface_name = format!("Api{}Request", method_base_name.to_pascal_case());

        let properties: Vec<TsProperty> = actual_params
            .iter()
            .map(|param| {
                let camel_case_name = param.name.to_lower_camel_case();
                TsProperty {
                    ts_name: camel_case_name.clone(),
                    original_name: camel_case_name,
                    type_expr: param.type_expr.clone(),
                    optional: param.optional,
                    is_index_signature: false,
                    documentation: None,
                }
            })
            .collect();

        let signature = TsInterfaceSignature::new(interface_name.clone(), interface_name);
        Some(TsInterfaceDefinition::new(signature).with_properties(properties))
    }

    /// Classify IR responses into success, error, and default buckets
    fn classify_ir_responses(
        responses: &[IrResponse],
    ) -> (
        BTreeMap<StatusCode, IrResponseWithContent>,
        BTreeMap<StatusCode, IrResponseWithContent>,
        Option<IrResponseWithContent>,
    ) {
        let mut success = BTreeMap::new();
        let mut errors = BTreeMap::new();
        let mut default = None;

        for ir_response in responses {
            let status = StatusCode::new(&ir_response.status);
            let wrapped = IrResponseWithContent {
                status: status.clone(),
                content: ir_response.content.clone(),
            };

            if status.is_default() {
                default = Some(wrapped);
            } else if status.is_success() {
                success.insert(status, wrapped);
            } else {
                errors.insert(status, wrapped);
            }
        }

        (success, errors, default)
    }

    /// Check if any IR response has a body
    fn ir_any_response_has_body(
        success: &BTreeMap<StatusCode, IrResponseWithContent>,
        errors: &BTreeMap<StatusCode, IrResponseWithContent>,
        default: &Option<IrResponseWithContent>,
    ) -> bool {
        success.values().any(|r| r.has_body())
            || errors.values().any(|r| r.has_body())
            || default.as_ref().is_some_and(|r| r.has_body())
    }

    /// Classify an IR response body by MIME type
    fn classify_ir_response_body(response: &IrResponseWithContent) -> IrResponseBodyKind {
        // Check for application/json first
        if let Some(type_expr) = response.content.get("application/json") {
            return IrResponseBodyKind::Json(Some(type_expr.clone()));
        }

        // Check for any JSON-like content type
        if response.content.keys().any(|k| k.contains("json")) {
            return IrResponseBodyKind::Json(None);
        }

        // Check for text-like content types
        let text_types = [
            "text/plain",
            "text/html",
            "application/xml",
            "text/xml",
            "application/x-www-form-urlencoded",
            "text/event-stream",
        ];
        if response
            .content
            .keys()
            .any(|k| text_types.iter().any(|t| k.contains(t)))
        {
            return IrResponseBodyKind::Text;
        }

        if response.has_body() {
            IrResponseBodyKind::Blob
        } else {
            IrResponseBodyKind::None
        }
    }

    /// Determine wrapper class for an IR response
    fn ir_wrapper_class(response: &IrResponseWithContent) -> String {
        match Self::classify_ir_response_body(response) {
            IrResponseBodyKind::Json(_) => "JSONApiResponse".to_string(),
            IrResponseBodyKind::Text => "TextApiResponse".to_string(),
            IrResponseBodyKind::Blob => "BlobApiResponse".to_string(),
            IrResponseBodyKind::None => "VoidApiResponse".to_string(),
        }
    }

    /// Build response expression for an IR response (e.g. `JSONApiResponse<Pet> & { status: 200 }`)
    fn ir_response_expression(response: &IrResponseWithContent) -> TsExpression {
        let status_type = response
            .status
            .literal()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "number".to_string());

        let response_expr = match Self::classify_ir_response_body(response) {
            IrResponseBodyKind::Json(Some(type_expr)) => {
                let ts_type = IrSchemaGenerator::type_expr_to_ts(&type_expr);
                let type_str = ts_type.to_string_formatted();
                format!(
                    "JSONApiResponse<{}> & {{ status: {} }}",
                    type_str, status_type
                )
            }
            IrResponseBodyKind::Json(None) => {
                format!("JSONApiResponse<any> & {{ status: {} }}", status_type)
            }
            IrResponseBodyKind::Text => {
                format!("TextApiResponse & {{ status: {} }}", status_type)
            }
            IrResponseBodyKind::Blob => {
                format!("BlobApiResponse & {{ status: {} }}", status_type)
            }
            IrResponseBodyKind::None => {
                format!("VoidApiResponse & {{ status: {} }}", status_type)
            }
        };

        TsExpression::Reference(response_expr)
    }

    /// Compute a schema transformer string for an IR response type
    fn ir_compute_schema_transformer(type_expr: &IrTypeExpr) -> Option<String> {
        match type_expr {
            IrTypeExpr::Named(name) => {
                let pascal_name = name.to_pascal_case();
                Some(format!("(jsonValue) => {}FromJSON(jsonValue)", pascal_name))
            }
            IrTypeExpr::Array(inner) => {
                if let IrTypeExpr::Named(name) = inner.as_ref() {
                    let pascal_name = name.to_pascal_case();
                    Some(format!(
                        "(jsonValue) => (jsonValue as Array<any>).map({}FromJSON)",
                        pascal_name
                    ))
                } else {
                    None
                }
            }
            IrTypeExpr::Nullable(inner) => Self::ir_compute_schema_transformer(inner),
            _ => None,
        }
    }

    /// Generate raw return type from IR responses
    fn generate_ir_raw_return_type(
        success: &BTreeMap<StatusCode, IrResponseWithContent>,
        errors: &BTreeMap<StatusCode, IrResponseWithContent>,
        default: &Option<IrResponseWithContent>,
        any_response_has_body: bool,
    ) -> TsExpression {
        let mut response_types: BTreeSet<TsExpression> = BTreeSet::new();

        for response in success.values() {
            response_types.insert(Self::ir_response_expression(response));
        }
        if let Some(default_response) = default {
            response_types.insert(Self::ir_response_expression(default_response));
        }
        for response in errors.values() {
            response_types.insert(Self::ir_response_expression(response));
        }
        if default.is_none() {
            let (_, fallback_expression) = fallback_response(any_response_has_body);
            response_types.insert(fallback_expression);
        }

        Self::wrap_in_promise(response_types, any_response_has_body)
    }

    /// Generate convenience return type from IR responses
    fn generate_ir_convenience_return_type(
        success: &BTreeMap<StatusCode, IrResponseWithContent>,
        errors: &BTreeMap<StatusCode, IrResponseWithContent>,
        default: &Option<IrResponseWithContent>,
        any_response_has_body: bool,
    ) -> TsExpression {
        let mut body_types: BTreeSet<TsExpression> = BTreeSet::new();

        let push_ir_body_type =
            |response: &IrResponseWithContent, body_types: &mut BTreeSet<TsExpression>| {
                match Self::classify_ir_response_body(response) {
                    IrResponseBodyKind::Json(Some(type_expr)) => {
                        body_types.insert(IrSchemaGenerator::type_expr_to_ts(&type_expr));
                    }
                    IrResponseBodyKind::Json(None) => {
                        body_types.insert(TsExpression::Primitive(TsPrimitive::Any));
                    }
                    IrResponseBodyKind::Text => {
                        body_types.insert(TsExpression::Primitive(TsPrimitive::String));
                    }
                    IrResponseBodyKind::Blob => {
                        body_types.insert(TsExpression::Reference("Blob".to_string()));
                    }
                    IrResponseBodyKind::None => {
                        if response.status.is_success() || response.status.is_default() {
                            body_types.insert(TsExpression::Primitive(TsPrimitive::Void));
                        } else {
                            body_types.insert(TsExpression::Primitive(TsPrimitive::Any));
                        }
                    }
                }
            };

        for response in success.values() {
            push_ir_body_type(response, &mut body_types);
        }
        if let Some(default_response) = default {
            push_ir_body_type(default_response, &mut body_types);
        }
        for response in errors.values() {
            push_ir_body_type(response, &mut body_types);
        }

        if body_types.is_empty() {
            if any_response_has_body {
                TsExpression::Primitive(TsPrimitive::Any)
            } else {
                TsExpression::Primitive(TsPrimitive::Void)
            }
        } else if body_types.len() == 1 {
            body_types.iter().next().cloned().unwrap()
        } else {
            TsExpression::Union(body_types)
        }
    }

    /// Wrap response types in a Promise
    fn wrap_in_promise(
        response_types: BTreeSet<TsExpression>,
        any_response_has_body: bool,
    ) -> TsExpression {
        if response_types.is_empty() {
            if any_response_has_body {
                TsExpression::Reference("Promise<JSONApiResponse<any>>".to_string())
            } else {
                TsExpression::Reference("Promise<VoidApiResponse>".to_string())
            }
        } else if response_types.len() == 1 {
            let type_str = response_types.iter().next().unwrap().to_string_formatted();
            TsExpression::Reference(format!("Promise<{}>", type_str))
        } else {
            let union_expr = TsExpression::Union(response_types);
            let union_str = union_expr.to_string_formatted();
            TsExpression::Reference(format!("Promise<{}>", union_str))
        }
    }

    /// Build all response templates from IR responses
    fn build_ir_response_templates(
        success: &BTreeMap<StatusCode, IrResponseWithContent>,
        errors: &BTreeMap<StatusCode, IrResponseWithContent>,
        default: &Option<IrResponseWithContent>,
    ) -> ResponseTemplates {
        let mut success_templates: BTreeMap<String, ResponseTemplateData> = BTreeMap::new();
        for (status, response) in success {
            let template = Self::build_ir_response_template_data(response, true);
            if template.status_condition.is_some() {
                success_templates.insert(status.raw().to_uppercase(), template);
            }
        }

        let default_template = default.as_ref().map(|response| {
            Self::build_ir_response_template_data(response, response.status.is_success())
        });

        let mut error_templates: BTreeMap<String, ResponseTemplateData> = BTreeMap::new();
        for (status, response) in errors {
            let template = Self::build_ir_response_template_data(response, false);
            error_templates.insert(status.raw().to_uppercase(), template);
        }

        let any_has_body = success.values().any(|r| r.has_body())
            || errors.values().any(|r| r.has_body())
            || default.as_ref().is_some_and(|r| r.has_body());

        let fallback = if default.is_none() {
            let (wrapper_class, response_expression) = fallback_response(any_has_body);
            Some(ResponseTemplateData {
                status_code: "FALLBACK".to_string(),
                is_success: true,
                has_body: any_has_body,
                body_type: if any_has_body {
                    Some(TsExpression::Primitive(TsPrimitive::Any))
                } else {
                    None
                },
                status_condition: None,
                wrapper_class,
                response_type: response_expression,
                transformer: None,
            })
        } else {
            None
        };

        ResponseTemplates {
            success_responses: success_templates,
            error_responses: error_templates,
            default_response: default_template,
            fallback_response: fallback,
        }
    }

    /// Build a single response template data from an IR response
    fn build_ir_response_template_data(
        response: &IrResponseWithContent,
        is_success: bool,
    ) -> ResponseTemplateData {
        let wrapper_class = Self::ir_wrapper_class(response);
        let transformer = if wrapper_class == "JSONApiResponse" {
            response
                .content
                .get("application/json")
                .and_then(Self::ir_compute_schema_transformer)
        } else {
            None
        };

        let body_type = response
            .content
            .get("application/json")
            .map(IrSchemaGenerator::type_expr_to_ts);

        ResponseTemplateData {
            status_code: response.status.raw().to_string(),
            is_success,
            has_body: response.has_body(),
            body_type,
            status_condition: response.status.condition_expression(),
            wrapper_class,
            response_type: Self::ir_response_expression(response),
            transformer,
        }
    }

    /// Extract body model name from IR operation for ToJSON usage
    fn extract_ir_body_model_name(
        ir_op: &IrOperation,
        schemas: &IndexMap<String, IrSchema>,
    ) -> Option<String> {
        let rb = ir_op.request_body.as_ref()?;
        let type_expr = rb.content.get("application/json")?;

        // Get the named reference
        let name = Self::ir_type_expr_named_ref(type_expr)?;
        let pascal_name = name.to_pascal_case();

        // Check if the schema is an interface (has properties → needs ToJSON)
        let schema = schemas.get(&name)?;
        if Self::ir_schema_is_interface(schema) {
            Some(pascal_name)
        } else {
            None
        }
    }

    /// Extract the top-level named reference from an IrTypeExpr (unwrapping nullable)
    fn ir_type_expr_named_ref(type_expr: &IrTypeExpr) -> Option<String> {
        match type_expr {
            IrTypeExpr::Named(name) => Some(name.clone()),
            IrTypeExpr::Nullable(inner) => Self::ir_type_expr_named_ref(inner),
            _ => None,
        }
    }

    /// Check if an IR schema is an interface (object with properties, not a type alias)
    fn ir_schema_is_interface(schema: &IrSchema) -> bool {
        matches!(
            &schema.kind,
            IrSchemaKind::Object(_) | IrSchemaKind::Intersection(_) | IrSchemaKind::TaggedUnion(_)
        )
    }

    /// Collect model dependencies from a single IR operation
    fn collect_ir_operation_dependencies(
        ir_op: &IrOperation,
        schemas: &IndexMap<String, IrSchema>,
        deps: &mut IrModelDependencies,
    ) {
        // Collect from responses
        for ir_response in &ir_op.responses {
            for type_expr in ir_response.content.values() {
                Self::collect_ir_type_refs(type_expr, deps, true);
            }
        }

        // Collect from parameters
        for ir_param in &ir_op.parameters {
            Self::collect_ir_type_refs(&ir_param.type_expr, deps, false);
        }

        // Collect from request body
        if let Some(rb) = &ir_op.request_body {
            for type_expr in rb.content.values() {
                let names = Self::collect_ir_named_refs(type_expr);
                for name in &names {
                    let pascal_name = name.to_pascal_case();
                    deps.type_names.insert(pascal_name.clone());
                    // Request body models need ToJSON if they're interfaces
                    if let Some(schema) = schemas.get(name.as_str())
                        && Self::ir_schema_is_interface(schema)
                    {
                        deps.function_names.insert(format!("{}ToJSON", pascal_name));
                    }
                }
            }
        }
    }

    /// Collect type references from an IrTypeExpr, adding FromJSON for response types
    fn collect_ir_type_refs(
        type_expr: &IrTypeExpr,
        deps: &mut IrModelDependencies,
        add_from_json: bool,
    ) {
        match type_expr {
            IrTypeExpr::Named(name) => {
                let pascal_name = name.to_pascal_case();
                deps.type_names.insert(pascal_name.clone());
                if add_from_json {
                    deps.function_names
                        .insert(format!("{}FromJSON", pascal_name));
                }
            }
            IrTypeExpr::Array(inner) => {
                Self::collect_ir_type_refs(inner, deps, add_from_json);
            }
            IrTypeExpr::Nullable(inner) => {
                Self::collect_ir_type_refs(inner, deps, add_from_json);
            }
            IrTypeExpr::Map(inner) => {
                Self::collect_ir_type_refs(inner, deps, add_from_json);
            }
            IrTypeExpr::Union(members) => {
                for member in members {
                    Self::collect_ir_type_refs(member, deps, add_from_json);
                }
            }
            IrTypeExpr::Primitive(_)
            | IrTypeExpr::StringLiteral(_)
            | IrTypeExpr::StringEnum(_)
            | IrTypeExpr::Any => {}
        }
    }

    /// Collect all Named references from an IrTypeExpr
    fn collect_ir_named_refs(type_expr: &IrTypeExpr) -> Vec<String> {
        let mut refs = Vec::new();
        match type_expr {
            IrTypeExpr::Named(name) => refs.push(name.clone()),
            IrTypeExpr::Array(inner) | IrTypeExpr::Nullable(inner) | IrTypeExpr::Map(inner) => {
                refs.extend(Self::collect_ir_named_refs(inner));
            }
            IrTypeExpr::Union(members) => {
                for member in members {
                    refs.extend(Self::collect_ir_named_refs(member));
                }
            }
            IrTypeExpr::Primitive(_)
            | IrTypeExpr::StringLiteral(_)
            | IrTypeExpr::StringEnum(_)
            | IrTypeExpr::Any => {}
        }
        refs
    }

    /// Build import statements from IR model dependencies
    fn build_ir_model_imports(deps: &IrModelDependencies) -> Vec<ApiImportStatement> {
        let mut models_by_file: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();

        for pascal_name in &deps.type_names {
            let filename = format!("../models/{}", pascal_name);
            let entry = models_by_file
                .entry(filename)
                .or_insert_with(|| (Vec::new(), Vec::new()));
            entry.0.push(pascal_name.clone());
        }

        for func_name in &deps.function_names {
            if let Some(model_name) = func_name
                .strip_suffix("FromJSON")
                .or_else(|| func_name.strip_suffix("ToJSON"))
            {
                let filename = format!("../models/{}", model_name);
                if let Some(entry) = models_by_file.get_mut(&filename) {
                    entry.1.push(func_name.clone());
                }
            }
        }

        let mut imports = Vec::new();
        let mut processed_files = BTreeSet::new();

        for (file_path, (type_names, func_names)) in &models_by_file {
            processed_files.insert(file_path.clone());
            let mut import_stmt = ApiImportStatement::new(file_path.clone());

            for pascal_type_name in type_names {
                import_stmt = import_stmt.with_type_import(pascal_type_name.clone(), None);
            }

            for func_name in func_names {
                import_stmt = import_stmt.with_import(func_name.clone(), None);
            }

            if !import_stmt.imports.is_empty() {
                imports.push(import_stmt);
            }
        }

        // Handle functions without corresponding type imports
        for func_name in &deps.function_names {
            if let Some(model_name) = func_name
                .strip_suffix("FromJSON")
                .or_else(|| func_name.strip_suffix("ToJSON"))
            {
                let filename = format!("../models/{}", model_name);
                if !processed_files.contains(&filename) {
                    imports.push(
                        ApiImportStatement::new(filename).with_import(func_name.clone(), None),
                    );
                }
            }
        }

        imports
    }

    /// Build method documentation from IR operation
    fn build_ir_method_documentation(
        ir_op: &IrOperation,
        parameters: &[TsParameter],
    ) -> Option<TsDocComment> {
        let mut doc_lines = Vec::new();

        if let Some(summary) = &ir_op.summary {
            doc_lines.push(summary.clone());
        } else if let Some(description) = &ir_op.description {
            doc_lines.push(description.clone());
        }

        // Collect parameter descriptions
        let mut param_descriptions: HashMap<String, String> = HashMap::new();
        for ir_param in &ir_op.parameters {
            if let Some(desc) = &ir_param.description {
                param_descriptions.insert(ir_param.name.clone(), desc.clone());
            }
        }
        if let Some(rb) = &ir_op.request_body
            && let Some(desc) = &rb.description
        {
            param_descriptions.insert("body".to_string(), desc.clone());
        }

        // Build @param annotations
        let mut jsdoc_params = Vec::new();
        for param in parameters {
            if param.name == "initOverrides" {
                continue;
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

        let mut throws = Vec::new();
        let has_required_params = parameters
            .iter()
            .any(|p| !p.optional && p.name != "initOverrides");
        if has_required_params {
            throws.push(("RequiredError".to_string(), String::new()));
        }

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

    /// Check for duplicate method names in IR operations
    fn check_duplicate_ir_method_names(&self, operations: &[&IrOperation], class_name: &str) {
        let mut used_raw_names: BTreeSet<String> = BTreeSet::new();
        let mut used_convenience_names: BTreeSet<String> = BTreeSet::new();

        for ir_op in operations {
            let base_method_name = ir_op.operation_id.to_lower_camel_case();
            let raw_method_name = format!("{}Raw", base_method_name);

            if used_raw_names.contains(&raw_method_name) {
                error!(
                    "Duplicate method name detected: '{}' for operation {} {} in API class '{}'. This will cause TypeScript compilation errors.",
                    raw_method_name, ir_op.method, ir_op.path, class_name
                );
            }
            used_raw_names.insert(raw_method_name);

            if used_convenience_names.contains(&base_method_name) {
                error!(
                    "Duplicate convenience method name detected: '{}' for operation {} {} in API class '{}'. This will cause TypeScript compilation errors.",
                    base_method_name, ir_op.method, ir_op.path, class_name
                );
            }
            used_convenience_names.insert(base_method_name);
        }
    }

    /// Determine which template to use based on HTTP method
    fn determine_template_name(method: &Method) -> Result<TemplateName, GeneratorError> {
        match *method {
            Method::GET => Ok(TemplateName::ApiMethodGet),
            Method::POST | Method::PUT | Method::PATCH => Ok(TemplateName::ApiMethodPostPutPatch),
            Method::DELETE => Ok(TemplateName::ApiMethodDelete),
            _ => Err(GeneratorError::UnsupportedHttpMethod {
                method: method.clone(),
            }),
        }
    }
}

// =============================================================================
// IR helper types
// =============================================================================

/// Extracted parameters from an IR operation
struct IrExtractedParameters {
    path_params: Vec<ParameterInfo>,
    query_params: Vec<ParameterInfo>,
    header_params: Vec<ParameterInfo>,
    body_param: Option<ParameterInfo>,
    /// Maps original param name → IrTypeExpr for TsParameter generation
    ir_param_types: HashMap<String, IrTypeExpr>,
}

/// Collected model dependencies for IR-based API generation
struct IrModelDependencies {
    type_names: BTreeSet<String>,
    function_names: BTreeSet<String>,
}

impl IrModelDependencies {
    fn new() -> Self {
        Self {
            type_names: BTreeSet::new(),
            function_names: BTreeSet::new(),
        }
    }
}

/// Wrapper around IR response data with parsed StatusCode
struct IrResponseWithContent {
    status: StatusCode,
    content: IndexMap<String, IrTypeExpr>,
}

impl IrResponseWithContent {
    fn has_body(&self) -> bool {
        !self.content.is_empty()
    }
}

/// Classification of an IR response body by content type
enum IrResponseBodyKind {
    Json(Option<IrTypeExpr>),
    Text,
    Blob,
    None,
}

// =============================================================================
// Free helper functions (inlined from deleted modules)
// =============================================================================

/// Convert a JSON value to a TypeScript literal string
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(obj) => {
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_string(v)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
    }
}

/// Returns the fallback response wrapper class name and type expression
/// for when no default response is defined.
fn fallback_response(any_response_has_body: bool) -> (String, TsExpression) {
    if any_response_has_body {
        (
            "JSONApiResponse".to_string(),
            TsExpression::Reference("JSONApiResponse<any> & { status: number }".to_string()),
        )
    } else {
        (
            "VoidApiResponse".to_string(),
            TsExpression::Reference("VoidApiResponse & { status: number }".to_string()),
        )
    }
}
