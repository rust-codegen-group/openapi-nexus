//! TypeScript code generation for OpenAPI specifications
//!
//! This crate provides TypeScript AST definitions and code generation
//! capabilities for OpenAPI 3.1 specifications.

pub mod ast;
pub mod codegen;
pub mod config;
pub mod consts;
pub mod emission;
pub mod errors;
pub mod generator;
pub mod templating;
pub mod utils;

// Re-export main types for convenience
pub use codegen::TypeScriptFetchCodeGenerator;
pub use config::{TypeScriptFetchConfig, TypeScriptModule};
