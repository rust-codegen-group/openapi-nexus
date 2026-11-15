use openapi_nexus::ir::types::{IrSchemaKind, IrTypeExpr};
use openapi_nexus::{ir, parser};

const FIXTURE: &str = include_str!("fixtures/valid/array-empty-items.yaml");

#[test]
fn empty_and_boolean_array_items_remain_unconstrained_in_every_version() {
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let source = FIXTURE.replacen("3.1.0", version, 1);
        let spec = ir::lower::lower(parser::parse_content_yaml(&source).unwrap()).unwrap();
        for schema_name in ["EmptyItems", "TrueItems"] {
            let IrSchemaKind::Object(object) = &spec.schemas[schema_name].kind else {
                panic!("{version}: {schema_name} must remain an object");
            };
            let values = &object.properties["values"].type_expr;
            assert_eq!(
                values,
                &IrTypeExpr::Array(Box::new(IrTypeExpr::Any)),
                "{version}: {schema_name} values"
            );
        }
    }
}
