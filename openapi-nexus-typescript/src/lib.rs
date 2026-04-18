//! TypeScript code generation for OpenAPI specifications
//!
//! This crate provides TypeScript AST definitions and code generation
//! capabilities for OpenAPI 3.1 specifications.

pub mod codegen;
pub mod config;
pub mod errors;
pub mod project_files;
pub mod sigil_emit;
pub mod sigil_emit_api;

// Re-export main types for convenience
pub use codegen::TypeScriptFetchCodeGenerator;
pub use config::{TypeScriptFetchConfig, TypeScriptModule};
