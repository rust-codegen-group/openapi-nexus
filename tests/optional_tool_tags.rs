use openapi_nexus::ir::lower::lower;
use openapi_nexus::ir::types::{IrSchemaKind, IrTypeExpr};
use openapi_nexus::parser::parse_content_yaml;

const FIXTURE: &str = include_str!("fixtures/valid/optional-tool-tags.yaml");

fn lower_version(version: &str) -> openapi_nexus::IrSpec {
    lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap()
}

#[test]
fn inferred_tool_tags_retain_optional_and_referenced_enum_facts() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower_version(version);
        let IrSchemaKind::TaggedUnion(union) = &ir.schemas["Tool"].kind else {
            panic!("{version}: Tool should retain inferred tagging");
        };
        assert_eq!(union.discriminator_field, "type", "{version}");
        let custom = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "custom")
            .unwrap();
        assert!(
            custom.discriminator_optional,
            "{version}: custom tag is optional"
        );
        assert!(custom.discriminator_values.is_empty());

        let search = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "SearchTool")
            .unwrap();
        assert!(!search.discriminator_optional, "{version}");
        assert_eq!(
            search.discriminator_values,
            ["web_search_20250305", "web_search_20260209"]
        );
        assert!(matches!(search.content_type, IrTypeExpr::Named(_)));
    }
}
