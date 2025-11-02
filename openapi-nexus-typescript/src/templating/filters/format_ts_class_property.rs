//! Template filter for formatting ClassProperty as TypeScript string

use minijinja::value::ViaDeserialize;

use crate::ast::TsClassProperty;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting ClassProperty as TypeScript string
pub fn format_ts_class_property_filter(
    property: ViaDeserialize<TsClassProperty>,
    indent_level: Option<usize>,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };
    property
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
