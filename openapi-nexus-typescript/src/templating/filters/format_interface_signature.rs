//! Template filter for formatting TsInterfaceSignature as TypeScript string

use minijinja::value::ViaDeserialize;

use crate::ast::TsInterfaceSignature;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting TsInterfaceSignature as a single-line string
pub fn format_interface_signature_filter(
    signature: ViaDeserialize<TsInterfaceSignature>,
    indent_level: Option<usize>,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };
    signature
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .unwrap_or_else(|_| "interface".to_string())
}
