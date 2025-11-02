//! Template filter for formatting a TypeScript method signature from TsClassMethod

use minijinja::value::ViaDeserialize;

use crate::ast::TsClassMethod;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting method signature (no body)
pub fn format_method_signature_filter(
    method: ViaDeserialize<TsClassMethod>,
    indent_level: Option<usize>,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };
    method
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .unwrap_or_else(|_| "/* invalid method */".to_string())
}
