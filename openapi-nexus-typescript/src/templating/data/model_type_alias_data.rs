//! Model type alias data for template generation

use serde::Serialize;

use crate::ast::ty::TsTypeAliasDefinition;

/// Model type alias data for template context
#[derive(Debug, Clone, Serialize)]
pub struct ModelTypeAliasData {
    pub type_alias_definition: TsTypeAliasDefinition,
}

impl ModelTypeAliasData {
    /// Create new model type alias data
    pub fn new(type_alias_definition: TsTypeAliasDefinition) -> Self {
        Self {
            type_alias_definition,
        }
    }
}
