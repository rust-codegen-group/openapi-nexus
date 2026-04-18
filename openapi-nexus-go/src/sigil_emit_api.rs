//! Sigil-stitch emit for IR operations (Go APIs).
//!
//! Groups operations by tag, emits one `apis/<tag>.go` in package `apis`.
//! Each file declares a `{Tag}API` struct carrying a `*runtime.Client` and
//! exposes one method per operation. Responses are typed per-operation
//! (`{OperationID}Response` with `StatusCode`, `Raw *http.Response`, typed
//! payload fields).

use openapi_nexus_core::traits::file_writer::FileInfo;
use openapi_nexus_ir::types::IrSpec;

/// Generate every API file from the IR.
///
/// Stub: not yet implemented. Returns an empty list so the rest of the
/// pipeline can be wired end-to-end.
pub fn generate_api_files(_ir: &IrSpec, _module_path: &str, _header: &str) -> Result<Vec<FileInfo>, String> {
    Ok(Vec::new())
}
