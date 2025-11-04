//! AST transformation passes for OpenAPI code generation
//!
//! This crate provides a pipeline of transformation passes that can be
//! applied to OpenAPI specifications before code generation.

pub mod ir_context;
pub mod passes;
pub mod pipeline;

pub use ir_context::{CustomTypes, IrContext, SchemaAnalysis, TypeMappings};
pub use passes::{
    CircularReferenceDetectionPass, DependencyAnalysisPass, NamingConvention, NamingConventionPass,
    PathNormalizationPass, ReferenceResolutionPass, SchemaNormalizationPass, TransformError,
    TransformPass, TypeInferencePass,
};
pub use pipeline::TransformPipeline;
