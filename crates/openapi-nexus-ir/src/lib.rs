//! Intermediate representation for OpenAPI code generation.
//!
//! The public API is:
//! - [`lower`] — lowers an OpenAPI spec into [`types::IrSpec`]
//! - [`types`] — IR type definitions consumed by language generators

pub mod lower;
pub(crate) mod tagged_enum_pattern;
pub mod types;
