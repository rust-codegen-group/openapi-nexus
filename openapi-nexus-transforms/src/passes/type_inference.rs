//! Type inference transformation pass

use crate::ir_context::IrContext;

use super::{IrTransformPass, TransformError};

/// Type inference transformation pass
#[derive(Default)]
pub struct TypeInferencePass {
    pub strict_mode: bool,
}

impl TypeInferencePass {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IrTransformPass for TypeInferencePass {
    fn name(&self) -> &str {
        "type-inference"
    }

    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError> {
        tracing::debug!("Inferring types from OpenAPI schemas");

        use openapi_nexus_ir::Analyzer;

        // Get all schemas from the OpenAPI spec
        let schemas = Analyzer::get_all_schemas(&ir.openapi);

        for (name, schema_ref) in schemas {
            let inferred_type = match schema_ref {
                openapi_nexus_spec::oas31::spec::ObjectOrReference::Object(obj_schema) => {
                    if !obj_schema.one_of.is_empty() {
                        "oneOf"
                    } else if !obj_schema.all_of.is_empty() {
                        "allOf"
                    } else if obj_schema.items.is_some() {
                        "array"
                    } else if !obj_schema.properties.is_empty()
                        || obj_schema.additional_properties.is_some()
                    {
                        "object"
                    } else {
                        "unknown"
                    }
                }
                openapi_nexus_spec::oas31::spec::ObjectOrReference::Ref { .. } => "reference",
            };

            tracing::debug!("Inferred type for schema '{}': {}", name, inferred_type);
            ir.schema_analysis
                .schema_types
                .insert(name.clone(), inferred_type.to_string());
        }

        // Initialize type mappings for common languages
        if !ir
            .type_mappings
            .openapi_to_language
            .contains_key("typescript")
        {
            let mut ts_mappings = std::collections::HashMap::new();
            ts_mappings.insert("string".to_string(), "string".to_string());
            ts_mappings.insert("integer".to_string(), "number".to_string());
            ts_mappings.insert("number".to_string(), "number".to_string());
            ts_mappings.insert("boolean".to_string(), "boolean".to_string());
            ts_mappings.insert("object".to_string(), "Record<string, any>".to_string());
            ts_mappings.insert("array".to_string(), "any[]".to_string());
            ir.type_mappings
                .openapi_to_language
                .insert("typescript".to_string(), ts_mappings);
        }

        if !ir.type_mappings.openapi_to_language.contains_key("rust") {
            let mut rust_mappings = std::collections::HashMap::new();
            rust_mappings.insert("string".to_string(), "String".to_string());
            rust_mappings.insert("integer".to_string(), "i32".to_string());
            rust_mappings.insert("number".to_string(), "f64".to_string());
            rust_mappings.insert("boolean".to_string(), "bool".to_string());
            rust_mappings.insert("object".to_string(), "serde_json::Value".to_string());
            rust_mappings.insert("array".to_string(), "Vec<serde_json::Value>".to_string());
            ir.type_mappings
                .openapi_to_language
                .insert("rust".to_string(), rust_mappings);
        }

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["schema-normalization", "reference-resolution"]
    }
}

#[cfg(test)]
mod tests {
    use super::{IrTransformPass, TypeInferencePass};
    // utoipa types available for tests if needed

    #[test]
    fn test_type_inference_pass_name() {
        let pass = TypeInferencePass::new();
        assert_eq!(pass.name(), "type-inference");
    }

    #[test]
    fn test_type_inference_pass_dependencies() {
        let pass = TypeInferencePass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["schema-normalization", "reference-resolution"]);
    }
}
