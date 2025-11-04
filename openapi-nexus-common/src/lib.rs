//! Common types for OpenAPI code generation
//!
//! This crate provides shared types used across the OpenAPI generator workspace,
//! particularly for error handling and source location tracking.

pub mod language;
pub mod location;
pub mod warning;

pub use language::Language;
pub use location::SourceLocation;
pub use warning::ParseWarning;
