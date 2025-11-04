//! OpenAPI specification parser
//!
//! This crate provides parsing functionality for OpenAPI specifications,
//! with a focus on YAML parsing and thorough error checking.

pub mod error;
pub mod parser;
pub mod serde_error;

pub use error::ParseError;
pub use parser::parse_file;
pub use serde_error::SerdeErrorExtractor;
