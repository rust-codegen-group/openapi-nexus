//! Template filter for formatting import statements

use minijinja::value::ViaDeserialize;

use crate::ast::TsImportStatement;
use crate::config::MAX_LINE_WIDTH;
use openapi_nexus_core::traits::{EmissionContext, ToRcDocWithContext};

/// Template filter for formatting import statements
pub fn format_import_filter(
    import: ViaDeserialize<TsImportStatement>,
    indent_level: Option<usize>,
) -> String {
    let ctx = EmissionContext {
        indent: indent_level.unwrap_or(0),
        max_line_width: MAX_LINE_WIDTH,
    };
    import
        .to_rcdoc_with_context(&ctx)
        .map(|doc| doc.pretty(MAX_LINE_WIDTH).to_string())
        .unwrap_or_else(|_| "import '???';".to_string())
}
