//! Project-level files for the requests client.

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::request_inputs::RequestInputModel;
use crate::ir::types::{IrInfo, IrOperation, IrSchema};
use indexmap::IndexMap;

/// Generate all requests project-level files.
pub fn generate_project_files(
    info: &IrInfo,
    package_name: &str,
    header: &str,
    schemas: &IndexMap<String, IrSchema>,
    operations: &[IrOperation],
    request_inputs: &[RequestInputModel],
    include_upload_file: bool,
) -> Vec<FileInfo> {
    super::super::project_files::generate_project_files(
        info,
        package_name,
        header,
        schemas,
        operations,
        request_inputs,
        include_upload_file,
        "requests>=2.32",
        std::iter::empty(),
    )
}
