//! Rust code generation for OpenAPI specifications
//!
//! This crate provides Rust AST definitions and code generation
//! capabilities for OpenAPI 3.1 specifications.

pub mod api_client;
pub mod ast;
pub mod emitter;
pub mod generator;
pub mod type_mapping;

pub use ast::*;
pub use emitter::{EmitError, RustEmitter};
pub use generator::{GeneratorError, RustGenerator};
