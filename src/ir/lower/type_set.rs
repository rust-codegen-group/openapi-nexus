//! Shared branch planning for OpenAPI 3.1/3.2 type-set lowering.
//!
//! Version adapters translate their schema-specific type enums and lower each
//! branch. This module owns the version-independent ordering, null wrapper,
//! scalar-enum metadata, and owner-scoped array naming policy.

use serde_json::Value;

use crate::ir::types::IrTypeExpr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TypeSetBranch {
    Object,
    Array,
    String,
    Integer,
    Number,
    Boolean,
    Null,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TypeSetPlanMode {
    /// Return no plan unless the node has multiple non-null type branches.
    Mixed,
    /// Lower the available branches even when the node has one non-null branch.
    Container,
}

/// Lower the branches of a normalized type set while keeping version-specific
/// schema access behind `lower_branch`.
pub(super) fn plan<E>(
    branches: &[TypeSetBranch],
    enum_values: &[Value],
    representation_is_constraint: bool,
    candidate_name: &str,
    mode: TypeSetPlanMode,
    mut lower_branch: impl FnMut(TypeSetBranch, &str) -> Result<IrTypeExpr, E>,
) -> Result<Option<IrTypeExpr>, E> {
    let non_null = branches
        .iter()
        .copied()
        .filter(|branch| *branch != TypeSetBranch::Null)
        .collect::<Vec<_>>();
    if matches!(mode, TypeSetPlanMode::Mixed) && non_null.len() <= 1 {
        return Ok(None);
    }

    let mut members = Vec::with_capacity(non_null.len());
    for branch in non_null {
        let branch_name = if branch == TypeSetBranch::Array {
            format!("{candidate_name}Item")
        } else {
            candidate_name.to_owned()
        };
        members.push(lower_branch(branch, &branch_name)?);
    }
    let base = if members.len() > 1 {
        IrTypeExpr::Union(members)
    } else {
        members
            .pop()
            .expect("type-set planning retains a non-null branch")
    };
    let base = if branches.contains(&TypeSetBranch::Null) {
        IrTypeExpr::Nullable(Box::new(base))
    } else {
        base
    };
    Ok(Some(base.with_scalar_enum(
        enum_values,
        representation_is_constraint,
    )))
}

#[cfg(test)]
mod tests {
    use super::{TypeSetBranch, TypeSetPlanMode, plan};
    use crate::ir::types::{IrPrimitive, IrTypeExpr};

    fn lower(branch: TypeSetBranch, candidate: &str) -> Result<IrTypeExpr, ()> {
        Ok(match branch {
            TypeSetBranch::Object => IrTypeExpr::Map(Box::new(IrTypeExpr::Any)),
            TypeSetBranch::Array => {
                IrTypeExpr::Array(Box::new(IrTypeExpr::Named(candidate.to_owned())))
            }
            TypeSetBranch::String => IrTypeExpr::Primitive(IrPrimitive::String),
            TypeSetBranch::Integer => IrTypeExpr::Primitive(IrPrimitive::Integer),
            TypeSetBranch::Number => IrTypeExpr::Primitive(IrPrimitive::Number),
            TypeSetBranch::Boolean => IrTypeExpr::Primitive(IrPrimitive::Boolean),
            TypeSetBranch::Null => unreachable!("null is wrapped by the planner"),
        })
    }

    #[test]
    fn mixed_plan_keeps_all_non_null_branches_and_nullability() {
        let actual = plan(
            &[
                TypeSetBranch::Object,
                TypeSetBranch::String,
                TypeSetBranch::Null,
            ],
            &[],
            true,
            "Owner",
            TypeSetPlanMode::Mixed,
            lower,
        )
        .unwrap();
        assert_eq!(
            actual,
            Some(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Union(vec![
                IrTypeExpr::Map(Box::new(IrTypeExpr::Any)),
                IrTypeExpr::Primitive(IrPrimitive::String),
            ]))))
        );
    }

    #[test]
    fn container_plan_supplies_owner_scoped_array_item_name() {
        let actual = plan(
            &[TypeSetBranch::Array],
            &[],
            true,
            "Owner",
            TypeSetPlanMode::Container,
            lower,
        )
        .unwrap();
        assert_eq!(
            actual,
            Some(IrTypeExpr::Array(Box::new(IrTypeExpr::Named(
                "OwnerItem".to_owned(),
            ))))
        );
    }

    #[test]
    fn mixed_mode_is_inert_for_a_single_non_null_branch() {
        assert_eq!(
            plan(
                &[TypeSetBranch::String, TypeSetBranch::Null],
                &[],
                true,
                "Owner",
                TypeSetPlanMode::Mixed,
                lower,
            )
            .unwrap(),
            None
        );
    }
}
