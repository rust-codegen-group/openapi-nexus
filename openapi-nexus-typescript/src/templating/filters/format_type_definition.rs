//! Template filter for formatting TypeDefinition as TypeScript string

use minijinja::value::ViaDeserialize;

use crate::ast::TsTypeDefinition;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting TypeDefinition as TypeScript string
pub fn format_type_definition_filter(
    type_def: ViaDeserialize<TsTypeDefinition>,
    indent_level: Option<usize>,
    max_line_width: usize,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width,
    };
    type_def
        .0
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(max_line_width).to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Create a format_type_definition filter with the given max_line_width
pub fn create_format_type_definition_filter(
    max_line_width: usize,
) -> impl Fn(ViaDeserialize<TsTypeDefinition>, Option<usize>) -> String + Send + Sync + 'static {
    move |type_def, indent_level| {
        format_type_definition_filter(type_def, indent_level, max_line_width)
    }
}
