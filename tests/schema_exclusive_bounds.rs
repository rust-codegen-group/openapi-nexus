use openapi_nexus::ir::types::IrSchemaKind;

#[test]
fn oas30_boolean_exclusive_bounds_retain_true_and_false() {
    for (literal, expected) in [("true", true), ("false", false), ("1", true)] {
        let yaml = format!(
            "openapi: 3.0.3\ninfo:\n  title: Bounds\n  version: '1'\npaths: {{}}\ncomponents:\n  schemas:\n    Bounds:\n      type: object\n      properties:\n        value:\n          type: number\n          minimum: 0\n          maximum: 10\n          exclusiveMinimum: {literal}\n          exclusiveMaximum: {literal}\n"
        );
        let parsed = openapi_nexus::parser::parse_content_yaml(&yaml).expect("bound schema parses");
        let ir = openapi_nexus::ir::lower::lower(parsed).expect("bound schema lowers");
        let IrSchemaKind::Object(object) = &ir.schemas["Bounds"].kind else {
            panic!("expected object");
        };
        let validation = object.properties["value"].validation.as_ref().unwrap();
        assert_eq!(validation.minimum, Some(0.0));
        assert_eq!(validation.maximum, Some(10.0));
        assert_eq!(validation.exclusive_minimum, Some(expected));
        assert_eq!(validation.exclusive_maximum, Some(expected));
    }
}
