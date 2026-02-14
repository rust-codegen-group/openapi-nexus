//! Response transformer computation for API operations

use heck::ToPascalCase as _;

use openapi_nexus_core::data::OperationInfo;
use openapi_nexus_spec::oas31::spec::{ObjectOrReference, ObjectSchema, Schema};

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
        if let Some(responses) = &op_info.operation.responses {
            for (status_code, response_ref) in responses {
                if !status_code.starts_with('2') {
                    continue;
                }
                if let ObjectOrReference::Object(response) = response_ref
                    && let Some(json_content) = response.content.get("application/json")
                    && let Some(schema_ref) = &json_content.schema
                {
                    match schema_ref {
                        ObjectOrReference::Ref { ref_path, .. } => {
                            if let Some(name) = ref_path.strip_prefix("#/components/schemas/") {
                                return Some(name.to_string());
                            }
                        }
                        ObjectOrReference::Object(object_schema) => {
                            // Check items for array of refs
                            if let Some(items) = &object_schema.items
                                && let Schema::Object(schema_ref) = items.as_ref()
                                && let ObjectOrReference::Ref { ref_path, .. } = schema_ref.as_ref()
                                && let Some(name) = ref_path.strip_prefix("#/components/schemas/")
                            {
                                return Some(name.to_string());
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
        schema_ref: &ObjectOrReference<ObjectSchema>,
    ) -> Option<String> {
        match schema_ref {
            ObjectOrReference::Ref { ref_path, .. } => {
                if let Some(name) = ref_path.strip_prefix("#/components/schemas/") {
                    let pascal_name = name.to_pascal_case();
                    Some(format!("(jsonValue) => {}FromJSON(jsonValue)", pascal_name))
                } else {
                    None
                }
            }
            ObjectOrReference::Object(object_schema) => {
                if let Some(items) = &object_schema.items
                    && let Schema::Object(schema_ref) = items.as_ref()
                    && let ObjectOrReference::Ref { ref_path, .. } = schema_ref.as_ref()
                    && let Some(name) = ref_path.strip_prefix("#/components/schemas/")
                {
                    let pascal_name = name.to_pascal_case();
                    return Some(format!(
                        "(jsonValue) => (jsonValue as Array<any>).map({}FromJSON)",
                        pascal_name
                    ));
                }
                None
            }
        }
    }
}

impl Default for ResponseTransformer {
    fn default() -> Self {
        Self::new()
    }
}
