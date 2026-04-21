//! Shared Rust code generation utilities for all Rust backend generators.
//!
//! Provides backend-agnostic model emission, API planning/emission, project
//! file helpers, and base configuration. Each Rust backend crate (reqwest,
//! ureq, aioduct) depends on this crate and plugs in its HTTP-specific
//! method body via a closure.

pub mod config;
pub mod emit_api;
pub mod emit_models;
pub mod project_files;
