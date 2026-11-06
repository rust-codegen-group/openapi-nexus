use openapi_nexus::ir::types::{IrPrimitive, IrSchemaKind, IrTypeExpr, TaggingStyle};
use openapi_nexus::{ir, parser};

#[test]
fn inferred_tagged_variants_retain_outer_additional_properties_in_every_version() {
    let fixture = include_str!("fixtures/valid/open-tagged-additional-properties.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for additional in ["true", "{type: string}"] {
            let source = fixture.replace("3.1.0", version).replace(
                "additionalProperties: true",
                &format!("additionalProperties: {additional}"),
            );
            let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
            let IrSchemaKind::TaggedUnion(union) = &spec.schemas["Event"].kind else {
                panic!("{version}: Event must remain a tagged union");
            };
            assert!(
                matches!(union.tagging, TaggingStyle::Internal),
                "{version}: {union:?}"
            );
            for variant in &union.variants {
                let IrTypeExpr::Named(name) = &variant.content_type else {
                    panic!("{version}: open variant must retain its complete object: {variant:?}");
                };
                let IrSchemaKind::Object(object) = &spec.schemas[name].kind else {
                    panic!("{version}: variant must be an object");
                };
                let expected = if additional == "true" {
                    IrTypeExpr::Any
                } else {
                    IrTypeExpr::Primitive(IrPrimitive::String)
                };
                assert_eq!(object.additional_properties, Some(expected), "{version}");
                if variant.discriminator_value == "searching" {
                    assert!(object.properties.contains_key("sequence_number"));
                }
            }
        }
    }
}

#[test]
fn closed_and_unspecified_tagged_variants_keep_the_existing_tagging_policy() {
    let fixture = include_str!("fixtures/valid/open-tagged-additional-properties.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for replacement in ["additionalProperties: false", ""] {
            let source = fixture
                .replace("3.1.0", version)
                .replace("additionalProperties: true", replacement);
            let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
            let IrSchemaKind::TaggedUnion(union) = &spec.schemas["Event"].kind else {
                panic!("{version}: Event must remain a tagged union");
            };
            assert!(
                matches!(&union.tagging, TaggingStyle::Adjacent { content_field } if content_field == "sequence_number")
            );
        }
    }
}

#[test]
fn open_reference_wrappers_keep_external_tagging_in_every_version() {
    let fixture = include_str!("fixtures/valid/open-tagged-additional-properties.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        for additional in ["true", "{type: string}"] {
            let source = fixture.replace("3.1.0", version).replace(
                "additionalProperties: true",
                &format!("additionalProperties: {additional}"),
            );
            let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
            let IrSchemaKind::TaggedUnion(union) = &spec.schemas["ExternalEvent"].kind else {
                panic!("{version}: external reference wrappers must remain tagged");
            };
            assert!(matches!(union.tagging, TaggingStyle::External));
            for variant in &union.variants {
                assert_eq!(
                    variant.content_type,
                    IrTypeExpr::Named("Payload".to_string())
                );
                let expected = if additional == "true" {
                    IrTypeExpr::Any
                } else {
                    IrTypeExpr::Primitive(IrPrimitive::String)
                };
                assert_eq!(variant.additional_properties, Some(expected));
            }
        }
    }
}
