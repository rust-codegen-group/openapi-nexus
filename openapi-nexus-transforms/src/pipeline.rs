//! Transformation pipeline for OpenAPI specifications

use utoipa::openapi::OpenApi;

use crate::passes::{TransformError, TransformPass};

/// Pipeline for applying multiple transformation passes
pub struct TransformPipeline {
    passes: Vec<Box<dyn TransformPass>>,
}

impl TransformPipeline {
    /// Create a new transformation pipeline
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Add a transformation pass to the pipeline
    pub fn add_pass<P: TransformPass + 'static>(mut self, pass: P) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// Apply all transformation passes to the OpenAPI specification
    pub fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError> {
        for pass in &self.passes {
            pass.transform(openapi).map_err(|e| {
                tracing::error!("Transform pass failed: {}", e);
                e
            })?;
        }
        Ok(())
    }
}

impl Default for TransformPipeline {
    fn default() -> Self {
        Self::new()
    }
}
