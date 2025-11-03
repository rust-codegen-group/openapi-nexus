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

    /// Compute JSON transformer expression if applicable
    pub fn compute_transformer(
        &self,
        _http_method: &Method,
        operation: &openapi::path::Operation,
    ) -> Option<String> {
        if let Some(model_name) = self.compute_model_name(_http_method, operation) {
            // Check if it's an array by looking at the schema
            for (status_code, response_ref) in operation.responses.responses.iter() {
                if !status_code.starts_with('2') {
                    continue;
                }
                if let openapi::RefOr::T(response) = response_ref
                    && let Some(json_content) = response.content.get("application/json")
                    && let Some(schema_ref) = &json_content.schema
                {
                    if let openapi::RefOr::T(schema) = schema_ref
                        && let Schema::Array(_) = schema
                    {
                        return Some(format!(
                            "(jsonValue) => (jsonValue as Array<any>).map({}FromJSON)",
                            model_name
                        ));
                    }
                    // Not an array, use direct transformer
                    return Some(format!("(jsonValue) => {}FromJSON(jsonValue)", model_name));
                }
            }
        }
        None
    }

    /// Compute model name from response schema if applicable
    pub fn compute_model_name(
        &self,
        _http_method: &Method,
        operation: &openapi::path::Operation,
    ) -> Option<String> {
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
                            return Some(name.to_string());
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
                                        return Some(name.to_string());
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

    /// Compute JSON transformer expression and model name if applicable
    ///
    /// This is kept for backwards compatibility but prefer using
    /// `compute_transformer` and `compute_model_name` separately.
    pub fn compute_transformer_and_model(
        &self,
        http_method: &Method,
        operation: &openapi::path::Operation,
    ) -> Option<(String, String)> {
        if let Some(model_name) = self.compute_model_name(http_method, operation)
            && let Some(transformer) = self.compute_transformer(http_method, operation)
        {
            return Some((transformer, model_name));
        }
        None
    }
}

impl Default for ResponseTransformer {
    fn default() -> Self {
        Self::new()
    }
}
