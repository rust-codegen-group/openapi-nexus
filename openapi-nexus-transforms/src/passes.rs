//! Transformation passes for OpenAPI specifications

use openapi_nexus_ir::OpenApi;
use snafu::Snafu;

use crate::ir_context::IrContext;

/// Error type for transformation passes
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum TransformError {
    #[snafu(display("Transform error: {}", message))]
    Generic { message: String },

    #[snafu(display("Transform pass '{}' failed: {}", pass, error))]
    PassFailed { pass: String, error: String },

    #[snafu(display("Circular dependency detected: {}", cycle))]
    CircularDependency { cycle: String },

    #[snafu(display("Invalid pass configuration: {}", message))]
    InvalidConfiguration { message: String },

    #[snafu(display("Pass '{}' not found", pass))]
    PassNotFound { pass: String },
}

/// OpenAPI-level transformation pass
/// These passes operate directly on the OpenAPI specification
pub trait OpenApiTransformPass {
    /// Get the name of this pass
    fn name(&self) -> &str;

    /// Apply the transformation to the OpenAPI specification
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError>;

    /// Get the names of passes that must run before this pass
    fn dependencies(&self) -> Vec<&str>;
}

/// IR-level transformation pass
/// These passes operate on the intermediate representation context
pub trait IrTransformPass {
    /// Get the name of this pass
    fn name(&self) -> &str;

    /// Apply the transformation to the IR context
    fn transform(&self, ir: &mut IrContext) -> Result<(), TransformError>;

    /// Get the names of passes that must run before this pass
    fn dependencies(&self) -> Vec<&str>;
}

/// AST-level transformation pass
/// These passes operate on language-specific ASTs
pub trait AstTransformPass<T> {
    /// Get the name of this pass
    fn name(&self) -> &str;

    /// Apply the transformation to the AST
    fn transform(&self, ast: &mut T) -> Result<(), TransformError>;

    /// Get the names of passes that must run before this pass
    fn dependencies(&self) -> Vec<&str>;
}

/// Base trait for transformation passes (for backward compatibility)
pub trait TransformPass {
    /// Apply the transformation to the OpenAPI specification
    fn transform(&self, openapi: &mut OpenApi) -> Result<(), TransformError>;
}

// Re-export all pass types from submodules
pub mod circular_reference_detection;
pub mod dependency_analysis;
pub mod naming_convention;
pub mod path_normalization;
pub mod reference_resolution;
pub mod schema_normalization;
pub mod type_inference;

pub use circular_reference_detection::CircularReferenceDetectionPass;
pub use dependency_analysis::DependencyAnalysisPass;
pub use naming_convention::NamingConventionPass;
pub use path_normalization::PathNormalizationPass;
pub use reference_resolution::ReferenceResolutionPass;
pub use schema_normalization::SchemaNormalizationPass;
pub use type_inference::TypeInferencePass;

// Re-export naming convention enum
pub use naming_convention::NamingConvention;
