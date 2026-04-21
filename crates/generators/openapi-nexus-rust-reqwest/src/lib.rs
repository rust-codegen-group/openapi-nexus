//! Rust reqwest code generator for OpenAPI specifications.
//!
//! This crate receives a pre-lowered `IrSpec` from `openapi-nexus-ir` and emits
//! idiomatic Rust using `sigil-stitch`. The generated SDK uses `reqwest` for HTTP,
//! `serde` for serialization, and `tokio` as the async runtime.

pub mod codegen;
pub mod config;
pub mod runtime;
pub mod sigil_emit;
pub mod sigil_emit_api;

pub use codegen::RustReqwestCodeGenerator;
pub use config::RustReqwestConfig;
