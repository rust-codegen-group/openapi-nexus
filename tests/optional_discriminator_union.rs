use openapi_nexus::ir::lower::lower;
use openapi_nexus::ir::types::{IrSchemaKind, IrTypeExpr};
use openapi_nexus::parser::parse_content_yaml;

const FIXTURE: &str = include_str!("fixtures/valid/optional-discriminator-union.yaml");

const SINGLETON_FIXTURE: &str = r##"
openapi: 3.1.0
info: {title: Singleton referenced discriminator, version: 1.0.0}
paths: {}
components:
  schemas:
    PayloadA:
      type: object
      required: [payload_a]
      properties:
        payload_a: {type: string}
    SingleValue:
      type: string
      enum: [variant_c]
    PayloadC:
      type: object
      required: [kind, payload_c]
      properties:
        kind: {$ref: '#/components/schemas/SingleValue'}
        payload_c: {type: string}
    Union:
      oneOf:
        - allOf:
            - {$ref: '#/components/schemas/PayloadA'}
            - type: object
              properties:
                kind: {type: string, enum: [variant_a]}
        - {$ref: '#/components/schemas/PayloadC'}
"##;

const INTERSECTION_FIXTURE: &str = r##"
openapi: 3.1.0
info: {title: Intersected discriminator, version: 1.0.0}
paths: {}
components:
  schemas:
    Union:
      oneOf:
        - allOf:
            - type: object
              properties:
                kind: {type: string, enum: [variant_a, variant_b]}
            - type: object
              properties:
                kind: {type: string, enum: [variant_a]}
        - type: object
          required: [kind]
          properties:
            kind: {type: string, enum: [variant_b]}
"##;

fn lower_version(version: &str) -> openapi_nexus::IrSpec {
    lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap()
}

#[test]
fn inferred_discriminators_retain_optional_and_referenced_enum_facts() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower_version(version);
        let IrSchemaKind::TaggedUnion(union) = &ir.schemas["Union"].kind else {
            panic!("{version}: Union should retain inferred tagging");
        };
        assert_eq!(union.discriminator_field, "kind", "{version}");
        let first = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "variant_a")
            .unwrap();
        assert!(
            first.discriminator_optional,
            "{version}: variant_a tag is optional"
        );
        assert!(first.discriminator_values.is_empty());

        let second = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "PayloadB")
            .unwrap();
        assert!(!second.discriminator_optional, "{version}");
        assert_eq!(second.discriminator_values, ["variant_b_1", "variant_b_2"]);
        assert!(matches!(second.content_type, IrTypeExpr::Named(_)));
    }
}

#[test]
fn inferred_union_retains_authored_reference_and_tag_property_before_projection() {
    use openapi_nexus::ir::types::IrWireShape;

    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower_version(version);
        let IrSchemaKind::TaggedUnion(union) = &ir.schemas["Union"].kind else {
            panic!("{version}: expected inferred union");
        };
        let authored = union
            .inferred_wire
            .as_ref()
            .expect("authored inferred branches");
        let IrWireShape::AllOf(parts) = &authored.branches[0] else {
            panic!("{version}: allOf should retain its distinct scopes");
        };
        assert!(matches!(&parts[0], IrWireShape::Reference(name) if name == "PayloadA"));
        let IrWireShape::Object(tag) = &parts[1] else {
            panic!("{version}: expected tag declaration");
        };
        assert!(tag.properties.contains_key("kind"));
        assert!(!tag.required.contains("kind"));
        assert!(
            matches!(&authored.branches[1], IrWireShape::Reference(name) if name == "PayloadB")
        );
        assert!(ir.wire_schemas.contains_key("PayloadB"));
    }
}

#[test]
fn referenced_singleton_and_allof_intersection_discriminators_remain_sound() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let singleton =
            lower(parse_content_yaml(&SINGLETON_FIXTURE.replacen("3.1.0", version, 1)).unwrap())
                .unwrap();
        let IrSchemaKind::TaggedUnion(union) = &singleton.schemas["Union"].kind else {
            panic!("{version}: singleton Union should retain inferred tagging");
        };
        let single = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "PayloadC")
            .unwrap();
        assert_eq!(single.discriminator_values, ["variant_c"]);

        let intersected =
            lower(parse_content_yaml(&INTERSECTION_FIXTURE.replacen("3.1.0", version, 1)).unwrap())
                .unwrap();
        let IrSchemaKind::TaggedUnion(union) = &intersected.schemas["Union"].kind else {
            panic!("{version}: intersected Union should retain inferred tagging");
        };
        let one = union
            .variants
            .iter()
            .find(|variant| variant.discriminator_value == "variant_a")
            .unwrap();
        assert!(
            one.discriminator_values.is_empty(),
            "{version}: allOf widened tag"
        );
    }
}
