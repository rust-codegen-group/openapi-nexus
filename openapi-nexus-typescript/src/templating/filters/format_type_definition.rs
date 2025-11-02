//! Template filter for formatting TypeDefinition as TypeScript string

use minijinja::value::ViaDeserialize;

use crate::ast::TsTypeDefinition;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting TypeDefinition as TypeScript string
pub fn format_type_definition_filter(
    type_def: ViaDeserialize<TsTypeDefinition>,
    indent_level: Option<usize>,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };
    type_def
        .0
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
