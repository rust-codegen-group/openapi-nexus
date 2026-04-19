//! Go HTTP code generator for OpenAPI specifications.
//!
//! This crate lowers an `OpenApiV31Spec` through `openapi-nexus-ir` and emits
//! idiomatic Go using `sigil-stitch`. The surface area is intentionally small:
//! one `sdk` package with functional-option construction, an `Authenticator`
//! interface for auth plumbing, typed response structs per operation, and plain
//! struct models with `json:` tags.

pub mod codegen;
pub mod config;
pub mod runtime;
pub mod sigil_emit;
pub mod sigil_emit_api;

pub use codegen::GoHttpCodeGenerator;
pub use config::GoHttpConfig;
