//! Response transformer computation for API operations

use utoipa::openapi;

use openapi_nexus_core::data::OperationInfo;
use openapi_nexus_core::traits::OpenApiRefExt as _;

/// Response transformer generator for JSON deserialization
#[derive(Debug, Clone)]
pub struct ResponseTransformer;

impl ResponseTransformer {
    /// Create a new response transformer
    pub fn new() -> Self {
        Self
    }

    /// Compute model name from response schema if applicable
    pub fn compute_model_name(&self, op_info: &OperationInfo) -> Option<String> {
        for (status_code, response_ref) in op_info.operation.responses.responses.iter() {
            if !status_code.starts_with('2') {
                continue;
            }
            if let openapi::RefOr::T(response) = response_ref
                && let Some(json_content) = response.content.get("application/json")
                && let Some(schema_ref) = &json_content.schema
            {
                match schema_ref {
                    openapi::RefOr::Ref(reference) => {
                        if let Some(name) = reference.schema_name() {
                            return Some(name.to_string());
                        }
                    }
                    openapi::RefOr::T(schema) => {
                        if let openapi::Schema::Array(arr) = schema {
                            match &arr.items {
                                openapi::schema::ArrayItems::RefOrSchema(item_ref) => {
                                    if let openapi::RefOr::Ref(reference) = &**item_ref
                                        && let Some(name) = reference.schema_name()
                                    {
                                        return Some(name.to_string());
                                    }
                                }
                                openapi::schema::ArrayItems::False => {}
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn compute_schema_transformer(
        &self,
        schema_ref: &openapi::RefOr<openapi::schema::Schema>,
    ) -> Option<String> {
        match schema_ref {
            openapi::RefOr::Ref(reference) => reference
                .schema_name()
                .map(|name| format!("(jsonValue) => {}FromJSON(jsonValue)", name)),
            openapi::RefOr::T(schema) => {
                if let openapi::Schema::Array(array_schema) = schema {
                    match &array_schema.items {
                        openapi::schema::ArrayItems::RefOrSchema(item_ref) => match &**item_ref {
                            openapi::RefOr::Ref(reference) => reference.schema_name().map(|name| {
                                format!(
                                    "(jsonValue) => (jsonValue as Array<any>).map({}FromJSON)",
                                    name
                                )
                            }),
                            openapi::RefOr::T(_) => None,
                        },
                        _ => None,
                    }
                } else {
                    None
                }
            }
        }
    }
}

impl Default for ResponseTransformer {
    fn default() -> Self {
        Self::new()
    }
}
