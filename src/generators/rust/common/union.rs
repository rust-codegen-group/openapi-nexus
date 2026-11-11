//! Typed untagged unions with enum-aware, policy-controlled deserialization.

use heck::{ToPascalCase, ToSnakeCase};
use sigil_stitch::error::SigilStitchError;
use sigil_stitch::lang::rust::Rust;
use sigil_stitch::prelude::{TypeName, sigil_quote};
use sigil_stitch::spec::{
    annotation_spec::AnnotationSpec,
    closed_sum_case_spec::ClosedSumCaseSpec,
    closed_sum_spec::ClosedSumSpec,
    file_spec::FileSpec,
    fun_spec::FunSpec,
    import_spec::ImportSpec,
    modifiers::{DeclarationContext, Visibility},
    parameter_spec::ParameterSpec,
    where_spec::GenericParamSpec,
};

use crate::generators::type_application;
use crate::generators::union_match::plan_union_members;
use crate::ir::types::{IrSchema, IrSpec, IrUnion};

use super::config::{ExtraDeriveConfig, RustGeneratorConfig, UntaggedUnionStrategy};
use super::emit_models::{build_utoipa_one_of_impl, rust_model_type, union_variant_name};
use super::matching::MatchPlan;

pub(super) fn emit_union(
    schema: &IrSchema,
    union: &IrUnion,
    extra: Option<&ExtraDeriveConfig>,
    config: &RustGeneratorConfig,
    ir: &IrSpec,
) -> Result<FileSpec, SigilStitchError> {
    let name = schema.name.to_pascal_case();
    let raw = config.untagged_union_strategy == UntaggedUnionStrategy::Raw;
    let utoipa = config.utoipa.as_ref().is_some_and(|utoipa| utoipa.enabled);
    let variants: Vec<_> = if raw {
        vec![(
            "Raw".to_string(),
            TypeName::qualified("serde_json", "Value"),
        )]
    } else {
        union
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| (union_variant_name(member, index), rust_model_type(member)))
            .collect()
    };

    let mut sum = ClosedSumSpec::builder(&name)
        .visibility(Visibility::Public)
        .annotate(derives(extra, raw, utoipa))
        .annotate(AnnotationSpec::new("serde").arg("untagged"));
    if let Some(description) = &schema.description {
        for line in description.lines() {
            sum = sum.doc(line);
        }
    }
    for (variant, payload) in &variants {
        sum = sum.add_case(ClosedSumCaseSpec::positional(
            variant,
            vec![payload.clone()],
        )?);
    }
    let mut file = FileSpec::builder(&format!("{}.rs", schema.name.to_snake_case()))
        .add_import(ImportSpec::named("serde", "Serialize"))
        .add_closed_sum(sum.build()?);

    if raw {
        file = file.add_import(ImportSpec::named("serde", "Deserialize"));
    }

    if !raw {
        let members = plan_union_members(&schema.name, union, ir);
        let mut matching = MatchPlan::default();
        for member in &members {
            matching.register(&member.shape, ir);
        }
        let strict = config.untagged_union_strategy == UntaggedUnionStrategy::Strict;
        let ambiguous = format!("ambiguous untagged union {name}: multiple concrete members match");
        let unmatched = format!("value does not match any member of untagged union {name}");
        let mut selection = Vec::new();
        for member in &members {
            let condition =
                matching.condition(&member.shape, sigil_quote!(RustLang { &candidate })?)?;
            let index = member.source_index;
            selection.push(if strict && !member.fallback {
                sigil_quote!(RustLang {
                    if $C(condition) {
                        if selected.is_some() { return Err(serde::de::Error::custom($S(&ambiguous))); }
                        selected = Some($L(index.to_string()));
                    }
                })?
            } else {
                sigil_quote!(RustLang {
                    if selected.is_none() && $C(condition) {
                        selected = Some($L(index.to_string()));
                    }
                })?
            });
        }
        let body = sigil_quote!(RustLang {
            let candidate = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
            $if(matching.is_recursive()) {
                let memo = &mut std::collections::HashMap::new();
            }
            let mut selected = None;
            $C_each(selection)
            match selected {
                $for((index, (variant, payload)) in variants.iter().enumerate()) {
                    Some($L(index.to_string())) => serde_json::from_value::<$T(payload.clone())>(candidate)
                        .map(Self::$N(variant))
                        .map_err(serde::de::Error::custom),
                }
                _ => Err(serde::de::Error::custom($S(unmatched))),
            }
        })?;
        // TypeArgument has no lifetime variant in sigil-stitch 0.7.5. Keep
        // only that atomic lifetime raw; the trait application stays semantic.
        let deserializer = type_application(
            TypeName::qualified("serde", "Deserializer"),
            vec![TypeName::raw("'de")],
        );
        let method = FunSpec::builder("deserialize")
            .add_generic_param(GenericParamSpec::single("D")?.with_bound(deserializer)?)
            .add_param(ParameterSpec::of("deserializer", TypeName::primitive("D")))
            .returns(vec![type_application(
                TypeName::primitive("Result"),
                vec![
                    TypeName::primitive("Self"),
                    TypeName::qualified("D", "Error"),
                ],
            )])
            .body(body)
            .build()?
            .emit(&Rust::new(), DeclarationContext::Member)?;
        // sigil-stitch has no trait-implementation spec. The method itself is
        // a FunSpec; the impl boundary is a structured quoted fragment.
        file = file.add_code(sigil_quote!(RustLang {
            impl<'de> serde::Deserialize<'de> for $N(&name) {
                $C(method)
            }
        })?);
        for function in matching.functions(ir)? {
            file = file.add_function(function);
        }
    }

    if utoipa {
        let payloads: Vec<_> = variants.into_iter().map(|(_, payload)| payload).collect();
        file = file.add_code(build_utoipa_one_of_impl(&name, &payloads)?);
    }
    file.build()
}

fn derives(extra: Option<&ExtraDeriveConfig>, raw: bool, utoipa: bool) -> AnnotationSpec {
    let mut names = vec!["Debug", "Clone", "Serialize"];
    if raw {
        names.push("Deserialize");
    }
    if let Some(extra) = extra {
        for derive in &extra.derives {
            // Serde traits are owned here, whether requested with their short
            // name or fully qualified. In typed mode Deserialize is manual.
            let path = derive.strip_prefix("::").unwrap_or(derive);
            let canonical = path.strip_prefix("serde::").unwrap_or(path);
            if canonical == "Deserialize" || names.contains(&canonical) {
                continue;
            }
            if utoipa && matches!(path, "utoipa::ToSchema" | "ToSchema") {
                // The integration owns a manual schema implementation for
                // typed unions and raw wrappers alike.
                continue;
            }
            names.push(derive);
        }
    }
    AnnotationSpec::new("derive").args(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::{IrPrimitive, IrSchemaKind, IrTypeExpr};
    use sigil_stitch::assert_rendered;

    fn fixture() -> IrSpec {
        crate::ir::lower::lower(
            crate::parser::parse_content_yaml(include_str!(
                "../../../../tests/fixtures/valid/union-selection.yaml"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn serde_derives_do_not_conflict_with_the_manual_trait_implementation() {
        let extra = ExtraDeriveConfig {
            derives: vec![
                "Deserialize".into(),
                "serde::Deserialize".into(),
                "serde::Serialize".into(),
                "::serde::Deserialize".into(),
                "::serde::Serialize".into(),
                "utoipa::ToSchema".into(),
                "::utoipa::ToSchema".into(),
                "PartialEq".into(),
            ],
            ..Default::default()
        };
        assert_rendered!(
            Rust::new(),
            derives(Some(&extra), false, true)
                .emit(&Rust::new())
                .unwrap(),
            "#[derive(Debug, Clone, Serialize, PartialEq)]\n",
        );
        assert_rendered!(
            Rust::new(),
            derives(Some(&extra), true, true)
                .emit(&Rust::new())
                .unwrap(),
            "#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]\n",
        );
    }

    #[test]
    fn nested_finite_metadata_keeps_the_inner_null_exclusion() {
        let mut ir = fixture();
        let inner = IrTypeExpr::ScalarEnum {
            representation: Box::new(IrTypeExpr::Nullable(Box::new(IrTypeExpr::Primitive(
                IrPrimitive::Integer,
            )))),
            representation_is_constraint: true,
            representation_nullable: false,
            values: vec![1.into()],
        };
        let outer = IrTypeExpr::ScalarEnum {
            representation: Box::new(inner),
            representation_is_constraint: true,
            representation_nullable: false,
            values: vec![1.into(), serde_json::Value::Null],
        };
        let IrSchemaKind::Object(object) = &mut ir.schemas.get_mut("NumericOne").unwrap().kind
        else {
            panic!("numeric member");
        };
        object.properties.get_mut("revision").unwrap().type_expr = outer;
        let members = plan_union_members(
            "Choice",
            match &ir.schemas["Choice"].kind {
                IrSchemaKind::Union(union) => union,
                _ => panic!("untagged choice"),
            },
            &ir,
        );
        let crate::generators::union_match::JsonShape::Object(properties) = &members[0].shape
        else {
            panic!("object guard");
        };
        // The inner finite guard remains a conjunct, not merely the outer
        // primitive representation plus its broader [1, null] enum.
        let revision = &properties
            .iter()
            .find(|property| property.wire_name == "revision")
            .unwrap()
            .shape;
        let crate::generators::union_match::JsonShape::AllOf(shapes) = revision else {
            panic!("nested constraints remain intersected");
        };
        assert!(
            shapes.contains(&crate::generators::union_match::JsonShape::Literal(
                1.into()
            ))
        );
    }
}
