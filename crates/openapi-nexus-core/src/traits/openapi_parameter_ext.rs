use openapi_nexus_spec::oas31::spec::{Components, ObjectOrReference, ObjectSchema, Parameter};

use super::openapi_ref_ext::COMPONENTS_PREFIX;

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
    fn default_value(&self, components: Option<&Components>) -> Option<serde_json::Value>;
}

impl OpenApiParameterExt for Parameter {
    fn required(&self) -> bool {
        self.required.unwrap_or(false)
    }

    fn deprecated(&self) -> bool {
        self.deprecated.unwrap_or(false)
    }

    fn default_value(&self, components: Option<&Components>) -> Option<serde_json::Value> {
        let schema_ref = self.schema.as_ref()?;
        extract_default_value_from_schema(schema_ref, components)
    }
}

/// Extract default value from a schema reference (helper for recursive resolution)
fn extract_default_value_from_schema(
    schema_ref: &ObjectOrReference<ObjectSchema>,
    components: Option<&Components>,
) -> Option<serde_json::Value> {
    match schema_ref {
        ObjectOrReference::Object(schema) => schema.default.clone(),
        ObjectOrReference::Ref { ref_path, .. } => {
            // Extract schema name from ref_path
            let schema_name = extract_component_name(ref_path, "schemas")?;
            components?
                .schemas
                .get(schema_name)
                .and_then(|resolved_schema| {
                    extract_default_value_from_schema(resolved_schema, components)
                })
        }
    }
}

fn extract_component_name<'a>(reference: &'a str, component: &str) -> Option<&'a str> {
    let remainder = reference.strip_prefix(COMPONENTS_PREFIX)?;
    let remainder = remainder.strip_prefix(component)?;
    remainder.strip_prefix('/')
}
