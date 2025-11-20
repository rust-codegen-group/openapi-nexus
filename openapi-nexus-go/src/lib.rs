//! Go code generation for OpenAPI specifications
//!
//! This crate provides Go code generation capabilities for OpenAPI 3.1 specifications.

pub mod ast;
pub mod config;
pub mod consts;
pub mod errors;
pub mod generator;
pub mod templating;
pub mod type_mapping;

// Re-export main types for convenience
pub use config::GoHttpConfig;
pub use generator::GoHttpCodeGenerator;
