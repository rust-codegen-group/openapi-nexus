//! Boolean component policy, independent of dependency traversal.
//!
//! Ordinary components remain public models even when no operation uses them.
//! A false component has no IR representation, so omit its declaration and reject
//! dependencies on it from the models and operations that are actually retained.

use std::collections::HashSet;

use super::LowerError;
use crate::ir::dependencies::SchemaDependencies;
use crate::ir::types::{IrSchema, IrSchemaKind, IrTypeExpr};

pub(super) fn lower(name: &str, value: bool) -> Option<IrSchema> {
    value.then(|| IrSchema {
        name: name.to_string(),
        description: None,
        deprecated: false,
        kind: IrSchemaKind::Alias(IrTypeExpr::Any),
        is_component: false,
    })
}

pub(super) fn validate(
    source: &impl SchemaDependencies,
    false_schema_names: &HashSet<String>,
) -> Result<(), LowerError> {
    if false_schema_names.is_empty() {
        return Ok(());
    }
    for name in source.schema_dependencies() {
        if false_schema_names.contains(name) {
            return Err(LowerError::Other {
                message: format!(
                    "boolean component schema `{name}` is false (rejects every value); the IR cannot represent an uninhabited type"
                ),
            });
        }
    }
    Ok(())
}
