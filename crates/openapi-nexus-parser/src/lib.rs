//! OpenAPI specification parser
//!
//! This crate provides parsing functionality for OpenAPI specifications,
//! with a focus on YAML parsing and thorough error checking.
//! Supports OpenAPI 3.0 and 3.1 with automatic version detection.

pub mod error;
pub mod parser;
pub mod serde_error;

pub use error::ParseError;
pub use parser::{
    parse_content_json, parse_content_json_v31, parse_content_yaml, parse_content_yaml_v31,
    parse_file,
};
pub use serde_error::SerdeErrorExtractor;

use openapi_nexus_spec::{OpenApiV30Spec, OpenApiV31Spec};

/// Parsed OpenAPI specification, version-tagged.
/// The parser auto-detects the version from the `openapi` field and
/// deserializes into the appropriate type.
#[derive(Debug)]
pub enum ParsedSpec {
    V30(Box<OpenApiV30Spec>),
    V31(Box<OpenApiV31Spec>),
}

impl ParsedSpec {
    /// Get the OpenAPI version string (e.g. "3.0" or "3.1").
    pub fn version_tag(&self) -> &'static str {
        match self {
            ParsedSpec::V30(_) => "3.0",
            ParsedSpec::V31(_) => "3.1",
        }
    }

    /// If this is a v3.1 spec, return a reference to it.
    pub fn as_v31(&self) -> Option<&OpenApiV31Spec> {
        match self {
            ParsedSpec::V31(spec) => Some(spec),
            _ => None,
        }
    }

    /// If this is a v3.0 spec, return a reference to it.
    pub fn as_v30(&self) -> Option<&OpenApiV30Spec> {
        match self {
            ParsedSpec::V30(spec) => Some(spec),
            _ => None,
        }
    }
}
