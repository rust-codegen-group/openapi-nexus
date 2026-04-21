//! Rust ureq code generator for OpenAPI specifications.
//!
//! This crate receives a pre-lowered `IrSpec` from `openapi-nexus-ir` and emits
//! idiomatic Rust using `ureq` for synchronous HTTP and `serde` for
//! serialization.

pub mod codegen;
pub mod config;
pub mod runtime;
pub mod sigil_emit_api;

pub use codegen::RustUreqCodeGenerator;
pub use config::RustUreqConfig;
