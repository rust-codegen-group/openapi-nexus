//! Template filter for formatting TsExpression as TypeScript string

use minijinja::value::ViaDeserialize;

use crate::ast::TsExpression;

/// Template filter for formatting TsExpression as TypeScript string
/// Uses Display implementation
pub fn format_ts_expression_filter(
    value: ViaDeserialize<TsExpression>,
) -> Result<String, minijinja::Error> {
    Ok(value.0.to_string())
}
