//! API emission for the requests transport.

use crate::codegen::traits::file_writer::FileInfo;
use crate::generators::python::emit_api::{self, Transport};
use crate::generators::python::value::UntaggedUnionStrategy;
use crate::generators::request_inputs::RequestInputPlan;
use crate::ir::types::IrSpec;

/// Generate every API file from the IR.
pub fn generate_api_files(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
) -> Vec<FileInfo> {
    generate_api_files_with_strategy(
        ir,
        header,
        request_inputs,
        UntaggedUnionStrategy::FirstMatch,
    )
}

/// Generate API files with the same union policy used by the model emitter.
pub fn generate_api_files_with_strategy(
    ir: &IrSpec,
    header: &str,
    request_inputs: &RequestInputPlan,
    strategy: UntaggedUnionStrategy,
) -> Vec<FileInfo> {
    emit_api::generate_api_files_with_strategy(
        ir,
        header,
        request_inputs,
        strategy,
        Transport::Requests,
    )
}
