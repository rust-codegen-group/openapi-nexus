//! Reference resolution transformation pass

use super::{OpenApiTransformPass, TransformError, TransformPass};
use openapi_nexus_ir::OpenApi;
use openapi_nexus_spec::oas31::spec::ObjectOrReference;

/// Reference resolution transformation pass
pub struct ReferenceResolutionPass;

impl Default for ReferenceResolutionPass {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceResolutionPass {
    pub fn new() -> Self {
        Self
    }
}

impl OpenApiTransformPass for ReferenceResolutionPass {
    fn name(&self) -> &str {
        "reference-resolution"
    }

    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        tracing::debug!("Resolving references");

        // Use ReferenceResolver from openapi-nexus-ir
        use openapi_nexus_ir::ReferenceResolver;

        let resolver = ReferenceResolver::new(&*openapi);

        // For now, just validate that references can be resolved
        // Full resolution would require deep cloning and replacement
        // which is complex with the utoipa types
        if let Some(components) = &openapi.components {
            for (name, schema_ref) in &components.schemas {
                if let ObjectOrReference::Ref { ref_path, .. } = schema_ref
                    && ref_path.starts_with("#/components/schemas/")
                {
                    let schema_name = ref_path.trim_start_matches("#/components/schemas/");
                    tracing::debug!("Found reference {} -> {}", name, schema_name);

                    if let Err(e) = resolver.resolve_schema_ref(ref_path) {
                        tracing::warn!("Invalid reference {}: {}", ref_path, e);
                    }
                }
            }
        }

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![]
    }
}

impl TransformPass for ReferenceResolutionPass {
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        <Self as OpenApiTransformPass>::transform(self, openapi)
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenApiTransformPass, ReferenceResolutionPass};
    use openapi_nexus_ir::OpenApi;

    #[test]
    fn test_reference_resolution_pass_name() {
        let pass = ReferenceResolutionPass::new();
        assert_eq!(pass.name(), "reference-resolution");
    }

    #[test]
    fn test_reference_resolution_pass_dependencies() {
        let pass = ReferenceResolutionPass::new();
        let deps = pass.dependencies();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_reference_resolution_pass_transform() {
        let pass = ReferenceResolutionPass::new();
        let yaml = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
"#;
        let mut openapi: OpenApi = openapi_nexus_parser::parse_content_yaml_v31(yaml).unwrap();

        // Should not panic or error on empty OpenAPI
        assert!(OpenApiTransformPass::transform(&pass, &mut openapi).is_ok());
    }
}
