//! Symbol-aware conversion at recursive model and tagged-payload boundaries.
//!
//! Both object and flattened allOf emitters use this path. Keeping references
//! structured lets the value plan choose a direct symbol or a cycle-safe module
//! consistently with annotations, without hand-maintained converter imports.

use sigil_stitch::prelude::*;

use super::emit_models::{is_object_schema, is_tagged_union_schema};
use super::value::{Direction, ValuePlan};
use crate::generators::type_application;
use crate::ir::types::{IrSpec, IrTypeExpr};

pub(super) fn model_conversion(
    expr: &IrTypeExpr,
    value: CodeBlock,
    ir: &IrSpec,
    plan: &ValuePlan<'_>,
    current: &str,
    direction: Direction,
) -> Option<CodeBlock> {
    let converted = match expr {
        IrTypeExpr::Named(name) if is_tagged_union_schema(name, ir) => {
            let helper = plan.tagged_helper(name, current, direction);
            let value = match direction {
                Direction::From => wire_value(dictionary_type(), value),
                Direction::To => value,
            };
            sigil_quote!(Python { $T(helper)($C(value)) })
        }
        IrTypeExpr::Named(name)
            if is_object_schema(name, ir) && plan.recursive_reference(name, current) =>
        {
            match direction {
                Direction::From => {
                    let model = plan.model_type(expr, current);
                    let value = wire_value(dictionary_type(), value);
                    sigil_quote!(Python { $T(model).from_dict($C(value)) })
                }
                Direction::To => sigil_quote!(Python { $C(value).to_dict() }),
            }
        }
        IrTypeExpr::Array(inner) => {
            let item = sigil_quote!(Python { item }).expect("Python recursive list item builds");
            let item = model_conversion(inner, item, ir, plan, current, direction)?;
            let value = match direction {
                Direction::From => wire_value(
                    type_application(
                        TypeName::primitive("list"),
                        vec![TypeName::primitive("object")],
                    ),
                    value,
                ),
                Direction::To => value,
            };
            sigil_quote!(Python { [$C(item) for item in $C(value)] })
        }
        IrTypeExpr::Map(inner) => {
            let item = sigil_quote!(Python { v }).expect("Python recursive map value builds");
            let item = model_conversion(inner, item, ir, plan, current, direction)?;
            let value = match direction {
                Direction::From => wire_value(dictionary_type(), value),
                Direction::To => value,
            };
            sigil_quote!(Python { {k: $C(item) for k, v in $C(value).items()} })
        }
        IrTypeExpr::Nullable(inner) => {
            let converted = model_conversion(inner, value.clone(), ir, plan, current, direction)?;
            sigil_quote!(Python { $C(converted) if $C(value) is not None else None })
        }
        // Leave unrelated legacy primitive and acyclic-object emission unchanged.
        _ => return None,
    };
    Some(converted.expect("Python recursive model value conversion builds"))
}

fn dictionary_type() -> TypeName {
    type_application(
        TypeName::primitive("dict"),
        vec![TypeName::primitive("str"), TypeName::primitive("object")],
    )
}

fn wire_value(ty: TypeName, value: CodeBlock) -> CodeBlock {
    sigil_quote!(Python { $T(TypeName::importable("typing", "cast"))($T(ty), $C(value)) })
        .expect("Python recursive model wire projection builds")
}

#[cfg(test)]
mod tests {
    use super::super::value::UntaggedUnionStrategy;
    use super::*;
    use sigil_stitch::{assert_rendered, lang::python::Python};

    #[test]
    fn recursive_tagged_list_conversion_tracks_module_and_cast_imports() {
        let ir = crate::ir::lower::lower(
            crate::parser::parse_content_yaml(include_str!(
                "../../../tests/fixtures/valid/python-tagged-cycle-symbols.yaml"
            ))
            .unwrap(),
        )
        .unwrap();
        let plan = ValuePlan::new(&ir, UntaggedUnionStrategy::FirstMatch);
        let expr = IrTypeExpr::Array(Box::new(IrTypeExpr::Named("Variant".into())));
        let converted = model_conversion(
            &expr,
            sigil_quote!(Python { data }).unwrap(),
            &ir,
            &plan,
            "VariantBranch",
            Direction::From,
        )
        .unwrap();
        assert_rendered!(
            Python::new(),
            width = 150,
            converted,
            "from typing import cast\n\nfrom . import variant\n\n[variant.variant_from_dict(cast(dict[str, object], item)) for item in cast(list[object], data)]\n"
        );
    }
}
