//! Rust aioduct code generator for OpenAPI specifications.
//!
//! This crate receives a pre-lowered `IrSpec` from `openapi-nexus-ir` and emits
//! idiomatic Rust using `aioduct` (hyper 1.x) for async HTTP with a generic
//! runtime parameter.

pub mod codegen;
pub mod config;
pub mod runtime;
pub mod sigil_emit_api;

pub use codegen::RustAioductCodeGenerator;
pub use config::RustAioductConfig;
