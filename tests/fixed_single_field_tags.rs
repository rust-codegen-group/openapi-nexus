use openapi_nexus::{
    generators::union_wire::{Selection, SelectionError, Strategy, UnionWirePlans},
    ir::{
        lower::lower,
        types::{IrSchemaKind, TaggingStyle},
    },
    parser::parse_content_yaml,
};
use serde_json::json;

const SOURCE: &str = include_str!("fixtures/valid/fixed-single-field-tags.yaml");

#[test]
fn a_shared_key_with_distinct_fixed_values_is_an_internal_discriminator() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = SOURCE.replacen("3.1.0", version, 1);
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        let IrSchemaKind::TaggedUnion(union) = &ir.schemas["FormatChoice"].kind else {
            panic!("{version}: expected a tagged union");
        };
        assert!(matches!(union.tagging, TaggingStyle::Internal), "{version}");
        assert_eq!(union.discriminator_field, "kind", "{version}");
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("FormatChoice").unwrap();
        assert!(plan.has_distinct_fixed_tags(), "{version}");
        for (index, label) in ["plain", "structured"].into_iter().enumerate() {
            assert_eq!(
                union.variants[index].discriminator_value, label,
                "{version}"
            );
            assert_eq!(
                plan.select(&json!({"kind": label})),
                Ok(Selection::Branch(index)),
                "{version}"
            );
        }
        for invalid in [
            json!({}),
            json!({"kind": "other"}),
            json!({"kind": ["plain"]}),
        ] {
            assert_eq!(
                plan.select(&invalid),
                Err(SelectionError::NoMatch),
                "{version}: {invalid}"
            );
        }
    }
}

#[test]
fn an_unproved_value_discriminator_does_not_reuse_the_same_external_key() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = SOURCE.replacen("3.1.0", version, 1);
        let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
        for name in [
            "ReferencedValues",
            "OverlappingValues",
            "NonStringSingletonLookalike",
            "SharedWrapper",
        ] {
            let IrSchemaKind::Union(union) = &ir.schemas[name].kind else {
                panic!("{version}: {name} must keep structural dispatch");
            };
            assert_eq!(union.members.len(), 2, "{version}: {name}");
        }
        let IrSchemaKind::TaggedUnion(external) = &ir.schemas["DistinctWrappers"].kind else {
            panic!("{version}: distinct keys still have external dispatch");
        };
        assert!(
            matches!(external.tagging, TaggingStyle::External),
            "{version}"
        );
        assert_eq!(
            external
                .variants
                .iter()
                .map(|variant| variant.discriminator_value.as_str())
                .collect::<Vec<_>>(),
            ["sized", "note"],
            "{version}"
        );

        let same_label = source.replacen("enum: [structured]", "enum: [plain]", 1);
        let ir = lower(parse_content_yaml(&same_label).unwrap()).unwrap();
        assert!(
            matches!(ir.schemas["FormatChoice"].kind, IrSchemaKind::Union(_)),
            "{version}"
        );

        let explicit = source.replacen(
            "    FormatChoice:\n",
            "    FormatChoice:\n      discriminator: {propertyName: kind}\n",
            1,
        );
        let ir = lower(parse_content_yaml(&explicit).unwrap()).unwrap();
        let IrSchemaKind::TaggedUnion(explicit) = &ir.schemas["FormatChoice"].kind else {
            panic!("{version}: explicit discriminator takes precedence");
        };
        assert!(explicit.inferred_wire.is_none(), "{version}");
    }
}

#[test]
fn a_string_enum_alone_does_not_prove_a_composed_discriminator() {
    let source = SOURCE.replacen("enum: [plain]}", "enum: [plain], const: blocked}", 1);
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    assert!(matches!(
        ir.schemas["FormatChoice"].kind,
        IrSchemaKind::Union(_)
    ));
}

#[test]
fn classifying_the_tag_does_not_relax_object_local_constraints() {
    let source = SOURCE.replacen(
        "          required: [kind]\n",
        "          required: [kind]\n          additionalProperties: false\n",
        1,
    );
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let plan = plans.get("FormatChoice").unwrap();
    assert_eq!(
        plan.select(&json!({"kind": "plain"})),
        Ok(Selection::Branch(0))
    );
    assert_eq!(
        plan.select(&json!({"kind": "plain", "extra": 1})),
        Err(SelectionError::NoMatch)
    );
    assert_eq!(
        plan.select(&json!({"kind": "structured", "extra": 1})),
        Ok(Selection::Branch(1))
    );
}
