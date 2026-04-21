//! Lowering pass: converts versioned OpenAPI specs into the version-agnostic IR.
//!
//! Entry point: `lower(parsed: ParsedSpec) -> Result<IrSpec, LowerError>`
//!
//! Dispatches to version-specific lowering functions that all produce the same `IrSpec`.

pub mod v30;
pub mod v31;
pub mod v32;

use crate::types::IrSpec;
use openapi_nexus_parser::ParsedSpec;

mod error;
pub use error::LowerError;

/// Lower a parsed OpenAPI spec (any supported version) into the IR.
pub fn lower(parsed: ParsedSpec) -> Result<IrSpec, LowerError> {
    lower_impl(&parsed)
}

fn lower_impl(parsed: &ParsedSpec) -> Result<IrSpec, LowerError> {
    match parsed {
        ParsedSpec::V30(spec) => v30::lower_v30(spec),
        ParsedSpec::V31(spec) => v31::lower_v31(spec),
        ParsedSpec::V32(spec) => v32::lower_v32(spec),
    }
}
