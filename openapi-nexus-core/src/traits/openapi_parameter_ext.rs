use utoipa::openapi;

/// Extension trait for OpenAPI `Parameter` to provide convenience methods.
pub trait OpenApiParameterExt {
    /// Returns `true` if the parameter is required.
    fn required(&self) -> bool;

    /// Returns `true` if the parameter is deprecated.
    fn deprecated(&self) -> bool;

    /// Extract default value from the parameter's schema.
    ///
    /// Returns `None` if the parameter has no schema or no default value is specified.
    /// For referenced schemas, resolves the reference using the provided `Components`.
    fn default_value(&self, components: Option<&openapi::Components>) -> Option<serde_json::Value>;
}

impl OpenApiParameterExt for openapi::path::Parameter {
    fn required(&self) -> bool {
        matches!(self.required, openapi::Required::True)
    }

    fn deprecated(&self) -> bool {
        matches!(self.deprecated, Some(openapi::Deprecated::True))
    }

    fn default_value(&self, components: Option<&openapi::Components>) -> Option<serde_json::Value> {
        let schema_ref = self.schema.as_ref()?;
        extract_default_value_from_schema(schema_ref, components)
    }
}

/// Extract default value from a schema reference (helper for recursive resolution)
fn extract_default_value_from_schema(
    schema_ref: &openapi::RefOr<openapi::Schema>,
    components: Option<&openapi::Components>,
) -> Option<serde_json::Value> {
    match (schema_ref, components) {
        (openapi::RefOr::T(openapi::Schema::Object(obj)), _) => obj.default.clone(),
        (openapi::RefOr::Ref(reference), Some(components)) => reference
            .ref_location
            .strip_prefix("#/components/schemas/")
            .and_then(|schema_name| components.schemas.get(schema_name))
            .and_then(|resolved_schema| {
                extract_default_value_from_schema(resolved_schema, Some(components))
            }),
        _ => None,
    }
}
