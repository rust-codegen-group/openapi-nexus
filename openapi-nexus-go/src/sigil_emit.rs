//! Sigil-stitch emit for IR schemas (Go models).
//!
//! Each supported `IrSchemaKind` maps to one `models/<name>.go` file in package
//! `models`. Covers Object (struct + json tags), Enum (typed string/int
//! constants), Alias (type alias), Union (sealed-interface marker),
//! Intersection (embedded-struct composition), TaggedUnion (discriminated
//! interface).

use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::IrSpec;

/// Generate every model file from the IR.
///
/// Stub: not yet implemented. Returns an empty list so the rest of the
/// pipeline can be wired end-to-end.
pub fn generate_model_files(_ir: &IrSpec, _header: &str) -> Result<Vec<FileInfo>, String> {
    Ok(Vec::new())
}
