//! Model type alias data for template generation

use serde::Serialize;

use crate::ast::ty::TsTypeAliasDefinition;
use crate::ast::ty::ts_type_alias_definition::UnionMemberInfo;
use crate::templating::data::ApiImportStatements;

/// Holds all information required by templates to render a TypeScript type alias model.
/// This includes data about union and intersection members for advanced type alias handling.
#[derive(Debug, Clone, Serialize)]
pub struct ModelTypeAliasData {
    /// The actual TypeScript type alias definition.
    pub type_alias_definition: TsTypeAliasDefinition,
    /// Map of imports needed by the generated model template (types and functions).
    /// Keyed by module_path for easy lookup and modification.
    /// Each import statement can contain both types (with inline `type` keyword) and values.
    pub imports: ApiImportStatements,
    /// Members of the union if this type alias is a union (e.g., oneOf/anyOf).
    /// - Each entry represents a single union member (e.g., interface or primitive type).
    /// - `None` if this is not a union type.
    pub union_members: Option<Vec<UnionMemberInfo>>,
}

impl ModelTypeAliasData {
    /// Create new model type alias data
    pub fn new(type_alias_definition: TsTypeAliasDefinition) -> Self {
        let union_members = type_alias_definition.union_members.clone();

        Self {
            union_members,
            type_alias_definition,
            imports: ApiImportStatements::new(),
        }
    }

    pub fn with_imports(mut self, imports: ApiImportStatements) -> Self {
        self.imports = imports;
        self
    }
}
