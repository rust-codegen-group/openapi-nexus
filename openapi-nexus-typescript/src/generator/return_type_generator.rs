//! Return type generation for API operations

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{TsExpression, TsPrimitive};
use crate::errors::GeneratorError;
use crate::utils::schema_mapper::SchemaMapper;
use openapi_nexus_spec::oas31::spec::Components;
use openapi_nexus_core::data::{ContentType, HttpResponse, OperationInfo, StatusCode};

/// Generator for API operation return types
#[derive(Debug, Clone)]
pub struct ReturnTypeGenerator;

/// Return type information for an API operation.
#[derive(Debug, Clone)]
pub struct ReturnTypeInfo {
    pub raw_return_type: TsExpression,
    pub convenience_return_type: TsExpression,
    pub success_responses: BTreeMap<StatusCode, HttpResponse>,
    pub error_responses: BTreeMap<StatusCode, HttpResponse>,
    pub default_response: Option<HttpResponse>,
}

impl ReturnTypeGenerator {
    pub fn generate_return_types(
        operation_info: &OperationInfo,
        components: Option<&Components>,
    ) -> Result<ReturnTypeInfo, GeneratorError> {
        let (success_responses, error_responses, default_response) =
            operation_info.collect_responses(components);

        let any_success_response_has_body = success_responses
            .values()
            .any(|response| response.has_body());
        let any_default_response_has_body = default_response
            .as_ref()
            .is_some_and(|response| response.has_body());
        let any_error_response_has_body =
            error_responses.values().any(|response| response.has_body());
        let any_response_has_body = any_success_response_has_body
            || any_default_response_has_body
            || any_error_response_has_body;

        let raw_return_type = Self::generate_raw_return_type(
            success_responses.values(),
            default_response.as_ref(),
            error_responses.values(),
            any_response_has_body,
        );

        let convenience_return_type = Self::generate_convenience_return_type(
            success_responses.values(),
            default_response.as_ref(),
            error_responses.values(),
            any_response_has_body,
        );

        Ok(ReturnTypeInfo {
            raw_return_type,
            convenience_return_type,
            success_responses,
            error_responses,
            default_response,
        })
    }

    fn generate_raw_return_type<'a>(
        success_responses: impl Iterator<Item = &'a HttpResponse>,
        default_response: Option<&'a HttpResponse>,
        error_responses: impl Iterator<Item = &'a HttpResponse>,
        any_response_has_body: bool,
    ) -> TsExpression {
        let mut response_types: BTreeSet<TsExpression> = BTreeSet::new();

        for response in success_responses {
            response_types.insert(Self::response_expression(response));
        }

        if let Some(default_response) = default_response {
            response_types.insert(Self::response_expression(default_response));
        }

        for response in error_responses {
            response_types.insert(Self::response_expression(response));
        }

        if default_response.is_none() {
            let (_, fallback_expression) = Self::fallback_response(any_response_has_body);
            response_types.insert(fallback_expression);
        }

        Self::wrap_in_promise(response_types, any_response_has_body)
    }

    fn generate_convenience_return_type<'a>(
        success_responses: impl Iterator<Item = &'a HttpResponse>,
        default_response: Option<&'a HttpResponse>,
        error_responses: impl Iterator<Item = &'a HttpResponse>,
        any_response_has_body: bool,
    ) -> TsExpression {
        let mut body_types: BTreeSet<TsExpression> = BTreeSet::new();

        for response in success_responses {
            Self::push_body_type(response, &mut body_types);
        }

        if let Some(default_response) = default_response {
            Self::push_body_type(default_response, &mut body_types);
        }

        for response in error_responses {
            Self::push_body_type(response, &mut body_types);
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

    pub(crate) fn response_expression(response: &HttpResponse) -> TsExpression {
        let status_type = response
            .status
            .literal()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "number".to_string());

        let response_expr = match Self::classify_response_body(response) {
            ResponseBodyKind::Json(Some(schema_ref)) => {
                let type_str =
                    SchemaMapper::map_ref_or_schema_to_type(schema_ref).to_string_formatted();
                format!(
                    "JSONApiResponse<{}> & {{ status: {} }}",
                    type_str, status_type
                )
            }
            ResponseBodyKind::Json(None) => {
                format!("JSONApiResponse<any> & {{ status: {} }}", status_type)
            }
            ResponseBodyKind::Text => {
                format!("TextApiResponse & {{ status: {} }}", status_type)
            }
            ResponseBodyKind::Blob => {
                format!("BlobApiResponse & {{ status: {} }}", status_type)
            }
            ResponseBodyKind::None => {
                format!("VoidApiResponse & {{ status: {} }}", status_type)
            }
        };

        TsExpression::Reference(response_expr)
    }

    pub fn fallback_response(any_response_has_body: bool) -> (String, TsExpression) {
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

    fn push_body_type(response: &HttpResponse, body_types: &mut BTreeSet<TsExpression>) {
        match Self::classify_response_body(response) {
            ResponseBodyKind::Json(Some(schema_ref)) => {
                body_types.insert(SchemaMapper::map_ref_or_schema_to_type(schema_ref));
            }
            ResponseBodyKind::Json(None) => {
                body_types.insert(TsExpression::Primitive(TsPrimitive::Any));
            }
            ResponseBodyKind::Text => {
                body_types.insert(TsExpression::Primitive(TsPrimitive::String));
            }
            ResponseBodyKind::Blob => {
                body_types.insert(TsExpression::Reference("Blob".to_string()));
            }
            ResponseBodyKind::None => {
                if response.is_success() {
                    body_types.insert(TsExpression::Primitive(TsPrimitive::Void));
                } else {
                    body_types.insert(TsExpression::Primitive(TsPrimitive::Any));
                }
            }
        }
    }

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

    fn classify_response_body<'a>(response: &'a HttpResponse) -> ResponseBodyKind<'a> {
        if let Some(schema_ref) = response.json_schema() {
            return ResponseBodyKind::Json(Some(schema_ref));
        }

        if response.has_json_body() {
            return ResponseBodyKind::Json(None);
        }

        if response.content_types().any(|content_type| {
            matches!(
                content_type,
                ContentType::Text
                    | ContentType::Html
                    | ContentType::Xml
                    | ContentType::FormUrlEncoded
                    | ContentType::TextEventStream
            )
        }) {
            return ResponseBodyKind::Text;
        }

        if response.has_body() {
            ResponseBodyKind::Blob
        } else {
            ResponseBodyKind::None
        }
    }
}

enum ResponseBodyKind<'a> {
    Json(Option<&'a openapi_nexus_spec::oas31::spec::ObjectOrReference<openapi_nexus_spec::oas31::spec::ObjectSchema>>),
    Text,
    Blob,
    None,
}
