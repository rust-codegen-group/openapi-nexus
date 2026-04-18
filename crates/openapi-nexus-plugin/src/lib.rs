//! Plugin system for OpenAPI code generation
//!
//! This crate defines the trait interfaces for extending the code generator
//! with custom language generators, transformation passes, and emitters.

pub mod registry;
pub mod traits;

pub use registry::SimplePluginRegistry;
pub use traits::{
    Emitter, Formatter, GeneratedFile, LanguageGenerator, Plugin, PluginCapability, PluginConfig,
    PluginError, PluginMetadata, PluginRegistry, TransformPass, ValidationError, ValidationResult,
    ValidationWarning, Validator,
};
