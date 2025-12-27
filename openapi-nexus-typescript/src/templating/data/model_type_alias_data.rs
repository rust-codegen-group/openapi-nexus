//! Model type alias data for template generation

use serde::Serialize;

use crate::ast::ty::TsTypeAliasDefinition;
use crate::ast::ty::ts_type_alias_definition::{IntersectionMemberInfo, UnionMemberInfo};
use crate::ast::{TsExpression, TsPrimitive};
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
    union_members: Option<Vec<UnionMemberInfo>>,
    /// Whether the union contains the `any` type.
    /// This is computed and exposed to templates for efficient checking.
    has_any_in_union: bool,
    /// Members of the intersection if this type alias is an intersection (e.g., allOf).
    /// - Each entry represents a single intersection member (e.g., reference or object type).
    /// - `None` if this is not an intersection type.
    intersection_members: Option<Vec<IntersectionMemberInfo>>,
}

impl ModelTypeAliasData {
    /// Create new model type alias data
    pub fn new(type_alias_definition: TsTypeAliasDefinition) -> Self {
        let union_members = type_alias_definition.union_members.clone();
        let has_any_in_union = union_members
            .as_ref()
            .map(|members| {
                members
                    .iter()
                    .any(|m| matches!(m.type_expr, TsExpression::Primitive(TsPrimitive::Any)))
            })
            .unwrap_or(false);
        let intersection_members = type_alias_definition.intersection_members.clone();

        Self {
            union_members,
            type_alias_definition,
            imports: ApiImportStatements::new(),
            has_any_in_union,
            intersection_members,
        }
    }

    pub fn with_imports(mut self, imports: ApiImportStatements) -> Self {
        self.imports = imports;
        self
    }

    /// Get union members if this is a union type.
    /// Returns `None` if this is not a union type.
    pub fn union_members(&self) -> Option<&[UnionMemberInfo]> {
        self.union_members.as_deref()
    }

    /// Check if the union contains the `any` type.
    pub fn has_any_in_union(&self) -> bool {
        self.has_any_in_union
    }

    /// Get intersection members if this is an intersection type.
    /// Returns `None` if this is not an intersection type.
    pub fn intersection_members(&self) -> Option<&[IntersectionMemberInfo]> {
        self.intersection_members.as_deref()
    }
}
