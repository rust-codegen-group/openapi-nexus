//! Circular reference detection transformation pass

use super::{IrTransformPass, TransformError};
use crate::ir_context::IrContext;
use openapi_nexus_ir::SchemaAnalyzer;

/// Circular reference detection transformation pass
pub struct CircularReferenceDetectionPass;

impl Default for CircularReferenceDetectionPass {
    fn default() -> Self {
        Self::new()
    }
}

impl CircularReferenceDetectionPass {
    pub fn new() -> Self {
        Self
    }
}

impl IrTransformPass for CircularReferenceDetectionPass {
    fn name(&self) -> &str {
        "circular-reference-detection"
    }

    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError> {
        tracing::debug!("Detecting circular references");

        // Use SchemaAnalyzer from openapi-nexus-ir
        let analyzer = SchemaAnalyzer::new(&ir.openapi);

        match analyzer.detect_circular_references() {
            Ok(circular_refs) => {
                if !circular_refs.is_empty() {
                    for ref_cycle in &circular_refs {
                        tracing::warn!(
                            "Circular reference detected: {}",
                            ref_cycle.path.join(" -> ")
                        );
                    }
                    // Store circular refs in IR context
                    ir.schema_analysis.circular_refs =
                        circular_refs.into_iter().map(|cr| cr.path).collect();
                }
            }
            Err(e) => {
                tracing::warn!("Error detecting circular references: {}", e);
            }
        }

        Ok(())
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["dependency-analysis"]
    }
}

#[cfg(test)]
mod tests {
    use super::{CircularReferenceDetectionPass, IrTransformPass};

    #[test]
    fn test_circular_reference_detection_pass_name() {
        let pass = CircularReferenceDetectionPass::new();
        assert_eq!(pass.name(), "circular-reference-detection");
    }

    #[test]
    fn test_circular_reference_detection_pass_dependencies() {
        let pass = CircularReferenceDetectionPass::new();
        let deps = pass.dependencies();
        assert_eq!(deps, vec!["dependency-analysis"]);
    }
}
