//! Naming convention transformation pass

use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use openapi_nexus_ir::OpenApi;

use super::{OpenApiTransformPass, TransformError, TransformPass};

/// Naming convention transformation pass
pub struct NamingConventionPass {
    pub target_case: NamingConvention,
}

#[derive(Debug)]
pub enum NamingConvention {
    CamelCase,
    PascalCase,
    SnakeCase,
    KebabCase,
}

impl OpenApiTransformPass for NamingConventionPass {
    fn name(&self) -> &str {
        "naming-convention"
    }

    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        tracing::debug!("Applying naming convention: {:?}", self.target_case);

        // Apply naming conventions to schema names
        if let Some(components) = openapi.components.as_mut() {
            let schemas = std::mem::take(&mut components.schemas);
            let mut normalized_schemas = std::collections::BTreeMap::new();

            for (name, schema) in schemas {
                let normalized_name = self.transform_name(&name);
                if name != normalized_name {
                    tracing::debug!("Renaming schema '{}' to '{}'", name, normalized_name);
                }
                normalized_schemas.insert(normalized_name, schema);
            }

            components.schemas = normalized_schemas;
        }

        // Transform path names
        let paths = openapi.paths.take().unwrap_or_default();
        let mut transformed_paths = std::collections::BTreeMap::new();

        for (path, path_item) in paths {
            let transformed_path = self.transform_path(&path);
            if path != transformed_path {
                tracing::debug!("Transforming path '{}' to '{}'", path, transformed_path);
            }
            transformed_paths.insert(transformed_path, path_item);
        }

        openapi.paths = Some(transformed_paths);

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["reference-resolution"]
    }
}

impl TransformPass for NamingConventionPass {
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        <Self as OpenApiTransformPass>::transform(self, openapi)
    }
}

impl NamingConventionPass {
    fn transform_name(&self, name: &str) -> String {
        match self.target_case {
            NamingConvention::CamelCase => name.to_lower_camel_case(),
            NamingConvention::PascalCase => name.to_pascal_case(),
            NamingConvention::SnakeCase => name.to_snake_case(),
            NamingConvention::KebabCase => name.to_kebab_case(),
        }
    }

    fn transform_path(&self, path: &str) -> String {
        // For paths, we typically want to keep them as-is or apply minimal transformation
        // This is a placeholder - in practice, path transformation might be more complex
        path.to_string()
    }
}
