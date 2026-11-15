//! Kotlin's transparent aliases have no JVM class on which to install a codec.
//! Opt in at their generated use sites, without changing the public Any API or
//! replacing Gson codecs for typed models and direct JsonElement dictionaries.

use std::collections::HashSet;

use sigil_stitch::prelude::*;

use crate::ir::types::{IrSchemaKind, IrSpec, IrTypeExpr};

/// True only for a wholly opaque alias path, including its list/map wrappers.
/// A typed object containing an opaque field still owns its ordinary codec.
pub(super) fn uses_alias(expr: &IrTypeExpr, ir: &IrSpec) -> bool {
    fn visit(expr: &IrTypeExpr, ir: &IrSpec, visiting: &mut HashSet<String>) -> Option<bool> {
        match expr {
            IrTypeExpr::Any => Some(false),
            IrTypeExpr::Array(inner) | IrTypeExpr::Map(inner) | IrTypeExpr::Nullable(inner) => {
                visit(inner, ir, visiting)
            }
            IrTypeExpr::Named(name) => {
                if !visiting.insert(name.clone()) {
                    return None;
                }
                let opaque = match ir.schemas.get(name).map(|schema| &schema.kind) {
                    Some(IrSchemaKind::Alias(inner)) => visit(inner, ir, visiting).map(|_| true),
                    _ => None,
                };
                visiting.remove(name);
                opaque
            }
            _ => None,
        }
    }
    visit(expr, ir, &mut HashSet::new()) == Some(true)
}

pub(super) fn is_needed(ir: &IrSpec) -> bool {
    ir.schemas.values().any(|schema| match &schema.kind {
        IrSchemaKind::Object(object) => {
            object
                .properties
                .values()
                .any(|property| uses_alias(&property.type_expr, ir))
                || object
                    .additional_properties
                    .as_ref()
                    .is_some_and(|values| uses_alias(values, ir))
        }
        _ => false,
    })
}

pub(super) fn annotation(package: &str) -> AnnotationSpec {
    // AnnotationSpec accepts textual arguments, not TypeName class literals.
    // Keep that renderer gap bounded to this qualified class-literal spelling.
    AnnotationSpec::importable(TypeName::qualified(
        "com.google.gson.annotations",
        "JsonAdapter",
    ))
    .args([
        format!("{package}.runtime.OpaqueJsonAdapterFactory::class"),
        "nullSafe = false".into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigil_stitch::{assert_rendered, lang::kotlin::Kotlin};

    #[test]
    fn opaque_alias_analysis_follows_containers_but_not_typed_boundaries_or_cycles() {
        let source = "openapi: 3.1.0\ninfo: {title: Alias facts, version: '1'}\npaths: {}\ncomponents:\n  schemas:\n    Anything: {type: object}\n    ListAlias: {type: array, items: true}\n    AliasChain: {$ref: '#/components/schemas/Anything'}\n    IntegerList: {type: array, items: {type: integer}}\n    Typed: {type: object, properties: {value: {$ref: '#/components/schemas/Anything'}}}\n";
        let mut ir =
            crate::ir::lower::lower(crate::parser::parse_content_yaml(source).unwrap()).unwrap();
        for name in ["Anything", "ListAlias", "AliasChain"] {
            let alias = IrTypeExpr::Named(name.into());
            assert!(uses_alias(&alias, &ir), "{name}");
            assert!(
                uses_alias(
                    &IrTypeExpr::Map(Box::new(IrTypeExpr::Array(Box::new(IrTypeExpr::Nullable(
                        Box::new(alias)
                    ),)))),
                    &ir
                ),
                "container {name}"
            );
        }
        for name in ["IntegerList", "Typed", "Missing"] {
            assert!(!uses_alias(&IrTypeExpr::Named(name.into()), &ir), "{name}");
        }
        assert!(!uses_alias(&IrTypeExpr::Any, &ir));
        assert!(!uses_alias(
            &IrTypeExpr::Array(Box::new(IrTypeExpr::Any)),
            &ir
        ));
        assert!(!uses_alias(
            &IrTypeExpr::Union(vec![
                IrTypeExpr::Named("Anything".into()),
                IrTypeExpr::Named("Typed".into())
            ]),
            &ir
        ));
        ir.schemas.get_mut("AliasChain").unwrap().kind =
            IrSchemaKind::Alias(IrTypeExpr::Named("AliasChain".into()));
        assert!(!uses_alias(&IrTypeExpr::Named("AliasChain".into()), &ir));
    }

    #[test]
    fn opaque_field_annotation_has_qualified_identities_and_exact_storage_target() {
        let property =
            ParameterSpec::builder("value", TypeName::qualified("sdk.models", "Anything"))
                .is_property()
                .annotate_storage(annotation("sdk"))
                .build()
                .unwrap();
        let declaration = TypeSpec::builder("Model", TypeKind::Struct)
            .visibility(Visibility::Public)
            .add_primary_constructor_param(property)
            .build()
            .unwrap()
            .emit(&Kotlin::new())
            .unwrap();
        let block = sigil_quote!(Kotlin { $C_each(declaration) }).unwrap();
        assert_rendered!(
            Kotlin::new(),
            width = 300,
            block,
            "data class Model(@field:com.google.gson.annotations.JsonAdapter(sdk.runtime.OpaqueJsonAdapterFactory::class, nullSafe = false)\n    val value: sdk.models.Anything) {\n}\n"
        );
    }
}
