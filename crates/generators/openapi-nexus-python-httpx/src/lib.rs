//! Python httpx code generator for OpenAPI specifications.
//!
//! This crate receives a pre-lowered `IrSpec` from `openapi-nexus-ir` and emits
//! a fully-typed Python package using `httpx` for HTTP and `dataclasses` for
//! models. Generated code targets Python 3.12+ and passes pyright strict mode.

pub mod codegen;
pub mod config;
pub mod emit_api;
pub mod emit_models;
pub mod project_files;
pub mod runtime;

pub use codegen::PythonHttpxCodeGenerator;
pub use config::PythonHttpxConfig;
