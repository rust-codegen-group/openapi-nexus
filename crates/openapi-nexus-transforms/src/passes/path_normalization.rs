//! Path normalization transformation pass

use openapi_nexus_ir::OpenApi;

use super::{OpenApiTransformPass, TransformError, TransformPass};

/// Path normalization transformation pass
pub struct PathNormalizationPass;

impl Default for PathNormalizationPass {
    fn default() -> Self {
        Self::new()
    }
}

impl PathNormalizationPass {
    pub fn new() -> Self {
        Self
    }
}

impl OpenApiTransformPass for PathNormalizationPass {
    fn name(&self) -> &str {
        "path-normalization"
    }

    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        tracing::debug!("Normalizing path patterns");

        let paths = openapi.paths.take().unwrap_or_default();
        let mut normalized_paths = std::collections::BTreeMap::new();

        for (mut path, path_item) in paths {
            if path.len() > 1 && path.ends_with('/') {
                path.pop();
            }
            if !path.starts_with('/') {
                path = format!("/{}", path);
            }
            normalized_paths.insert(path, path_item);
        }

        let len = normalized_paths.len();
        openapi.paths = Some(normalized_paths);

        tracing::debug!("Normalized {} paths", len);

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["reference-resolution"]
    }
}

impl TransformPass for PathNormalizationPass {
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        <Self as OpenApiTransformPass>::transform(self, openapi)
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenApiTransformPass, PathNormalizationPass};
    // utoipa types available for tests if needed

    #[test]
    fn test_path_normalization_pass_name() {
        let pass = PathNormalizationPass::new();
        assert_eq!(pass.name(), "path-normalization");
    }

    #[test]
    fn test_path_normalization_pass_dependencies() {
        let pass = PathNormalizationPass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["reference-resolution"]);
    }
}
