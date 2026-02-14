//! Schema normalization transformation pass

use openapi_nexus_ir::OpenApi;

use super::{OpenApiTransformPass, TransformError, TransformPass};

/// Schema normalization transformation pass
pub struct SchemaNormalizationPass {
    pub normalize_arrays: bool,
    pub normalize_objects: bool,
}

impl Default for SchemaNormalizationPass {
    fn default() -> Self {
        Self {
            normalize_arrays: true,
            normalize_objects: true,
        }
    }
}

impl SchemaNormalizationPass {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OpenApiTransformPass for SchemaNormalizationPass {
    fn name(&self) -> &str {
        "schema-normalization"
    }

    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        tracing::debug!("Normalizing schema structures");

        if let Some(components) = openapi.components.as_mut() {
            for (_name, schema_ref) in components.schemas.iter_mut() {
                if let openapi_nexus_spec::oas31::spec::ObjectOrReference::Object(obj_schema) = schema_ref {
                    if !obj_schema.properties.is_empty() && self.normalize_objects {
                        tracing::debug!("Normalizing object schema properties");
                    }
                    if obj_schema.items.is_some() && self.normalize_arrays {
                        tracing::debug!("Normalizing array schema");
                    }
                }
            }
        }

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["reference-resolution"]
    }
}

impl TransformPass for SchemaNormalizationPass {
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        <Self as OpenApiTransformPass>::transform(self, openapi)
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenApiTransformPass, SchemaNormalizationPass};
    // utoipa types available for tests if needed

    #[test]
    fn test_schema_normalization_pass_name() {
        let pass = SchemaNormalizationPass::new();
        assert_eq!(pass.name(), "schema-normalization");
    }

    #[test]
    fn test_schema_normalization_pass_dependencies() {
        let pass = SchemaNormalizationPass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["reference-resolution"]);
    }
}
