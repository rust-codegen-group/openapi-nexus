//! Dependency analysis transformation pass

use super::{IrTransformPass, TransformError};
use crate::ir_context::IrContext;

/// Dependency analysis transformation pass
pub struct DependencyAnalysisPass;

impl Default for DependencyAnalysisPass {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyAnalysisPass {
    pub fn new() -> Self {
        Self
    }
}

impl IrTransformPass for DependencyAnalysisPass {
    fn name(&self) -> &str {
        "dependency-analysis"
    }

    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError> {
        tracing::debug!("Analyzing schema dependencies");

        use openapi_nexus_ir::Analyzer;
        use openapi_nexus_ir::SchemaAnalyzer;

        // Get all schemas
        let schemas = Analyzer::get_all_schemas(&ir.openapi);
        let analyzer = SchemaAnalyzer::new(&ir.openapi);

        // Analyze dependencies for each schema
        for (name, _) in schemas {
            match analyzer.analyze_schema_dependencies(name) {
                Ok(dependencies) => {
                    if !dependencies.is_empty() {
                        tracing::debug!(
                            "Schema '{}' has {} dependencies",
                            name,
                            dependencies.len()
                        );
                        ir.schema_analysis
                            .dependencies
                            .insert(name.clone(), dependencies);
                    }
                }
                Err(e) => {
                    tracing::warn!("Error analyzing dependencies for schema '{}': {}", name, e);
                }
            }
        }

        tracing::debug!(
            "Analyzed dependencies for {} schemas",
            ir.schema_analysis.dependencies.len()
        );

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["type-inference"]
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyAnalysisPass, IrTransformPass};

    #[test]
    fn test_dependency_analysis_pass_name() {
        let pass = DependencyAnalysisPass::new();
        assert_eq!(pass.name(), "dependency-analysis");
    }

    #[test]
    fn test_dependency_analysis_pass_dependencies() {
        let pass = DependencyAnalysisPass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["type-inference"]);
    }
}
