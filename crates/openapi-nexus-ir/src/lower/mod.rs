//! Lowering pass: converts versioned OpenAPI specs into the version-agnostic IR.
//!
//! Public entry points:
//! - `lower(parsed: ParsedSpec) -> Result<IrSpec, LowerError>` (consumes)
//! - `lower_ref(parsed: &ParsedSpec) -> Result<IrSpec, LowerError>` (borrows)
//!
//! Dispatches to version-specific lowering functions that all produce the same `IrSpec`.

pub mod v31;

use crate::types::IrSpec;
use openapi_nexus_parser::ParsedSpec;

mod error;
pub use error::LowerError;

/// Lower a parsed OpenAPI spec (any supported version) into the IR.
pub fn lower(parsed: ParsedSpec) -> Result<IrSpec, LowerError> {
    lower_ref(&parsed)
}

/// Lower a parsed OpenAPI spec by reference into the IR.
pub fn lower_ref(parsed: &ParsedSpec) -> Result<IrSpec, LowerError> {
    match parsed {
        ParsedSpec::V31(spec) => v31::lower_v31(spec),
        ParsedSpec::V30(_) => Err(LowerError::UnsupportedVersion {
            version: "3.0".to_string(),
        }),
    }
}
