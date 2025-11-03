//! Response transformer computation for API operations

use http::Method;
use utoipa::openapi;
use utoipa::openapi::schema::{ArrayItems, Schema};

/// Response transformer generator for JSON deserialization
#[derive(Debug, Clone)]
pub struct ResponseTransformer;

impl ResponseTransformer {
    /// Create a new response transformer
    pub fn new() -> Self {
        Self
    }

    /// Compute JSON transformer expression and model name if applicable
    pub fn compute_transformer_and_model(
        &self,
        _http_method: &Method,
        operation: &openapi::path::Operation,
    ) -> Option<(String, String)> {
        for (status_code, response_ref) in operation.responses.responses.iter() {
            if !status_code.starts_with('2') {
                continue;
            }
            if let openapi::RefOr::T(response) = response_ref
                && let Some(json_content) = response.content.get("application/json")
                && let Some(schema_ref) = &json_content.schema
            {
                match schema_ref {
                    openapi::RefOr::Ref(reference) => {
                        if let Some(name) =
                            reference.ref_location.strip_prefix("#/components/schemas/")
                        {
                            let expr = format!("(jsonValue) => {}FromJSON(jsonValue)", name);
                            return Some((expr, name.to_string()));
                        }
                    }
                    openapi::RefOr::T(schema) => {
                        if let Schema::Array(arr) = schema {
                            match &arr.items {
                                ArrayItems::RefOrSchema(item_ref) => {
                                    if let openapi::RefOr::Ref(reference) = &**item_ref
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

impl Default for ResponseTransformer {
    fn default() -> Self {
        Self::new()
    }
}
