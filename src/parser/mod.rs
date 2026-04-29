pub mod error;
#[allow(clippy::module_inception)]
pub mod parser;
pub mod serde_error;

pub use error::ParseError;
pub use parser::{
    parse_content_json, parse_content_json_v31, parse_content_yaml, parse_content_yaml_v31,
    parse_file,
};
pub use serde_error::SerdeErrorExtractor;

use crate::spec::{OpenApiV30Spec, OpenApiV31Spec, OpenApiV32Spec};

#[derive(Debug)]
pub enum ParsedSpec {
    V30(Box<OpenApiV30Spec>),
    V31(Box<OpenApiV31Spec>),
    V32(Box<OpenApiV32Spec>),
}

impl ParsedSpec {
    pub fn version_tag(&self) -> &'static str {
        match self {
            ParsedSpec::V30(_) => "3.0",
            ParsedSpec::V31(_) => "3.1",
            ParsedSpec::V32(_) => "3.2",
        }
    }

    pub fn as_v32(&self) -> Option<&OpenApiV32Spec> {
        match self {
            ParsedSpec::V32(spec) => Some(spec),
            _ => None,
        }
    }

    pub fn as_v31(&self) -> Option<&OpenApiV31Spec> {
        match self {
            ParsedSpec::V31(spec) => Some(spec),
            _ => None,
        }
    }

    pub fn as_v30(&self) -> Option<&OpenApiV30Spec> {
        match self {
            ParsedSpec::V30(spec) => Some(spec),
            _ => None,
        }
    }
}
