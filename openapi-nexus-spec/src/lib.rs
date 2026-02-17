//! OpenAPI specification types for OpenAPI Nexus.
//!
//! This crate provides versioned modules for OpenAPI 3.0, 3.1, and 3.2.
//! OpenAPI 3.1 is fully implemented; 3.0 and 3.2 are stubs for future use.

pub mod oas30;
pub mod oas31;
pub mod oas32;

// Re-export 3.1 as the primary implementation.
pub use oas31::OpenApiV31Spec;
