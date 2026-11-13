use super::*;
use crate::ir::lower::lower;
use crate::parser::parse_content_yaml;

fn fixture(source: &str) -> IrSpec {
    lower(parse_content_yaml(source).unwrap()).unwrap()
}

fn snapshot(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn array_aliases_share_the_same_compatible_nested_projection() {
    let ir = fixture(include_str!(
        "../../../../../tests/fixtures/valid/typescript-overlapping-allof.yaml"
    ));
    let projected = project(&ir);
    let IrSchemaKind::Object(object) = &projected.schemas["ArrayOverlap"].kind else {
        panic!("array aliases must retain the same merged view as inline arrays");
    };
    assert!(matches!(
        &object.properties["config"].type_expr,
        IrTypeExpr::Array(_)
    ));
}

#[test]
fn projection_keeps_integer_selection_constraints_independent_of_member_order() {
    use crate::ir::types::IrPrimitive;
    let ir = fixture(include_str!(
        "../../../../../tests/fixtures/valid/typescript-overlapping-allof.yaml"
    ));
    for reverse in [false, true] {
        let mut ir = ir.clone();
        if reverse && let IrSchemaKind::Intersection(all) = &mut ir.schemas["NumericOverlap"].kind {
            all.members.reverse();
        }
        let projected = project(&ir);
        let IrSchemaKind::Object(object) = &projected.schemas["NumericOverlap"].kind else {
            panic!("object view")
        };
        assert_eq!(
            object.properties["count"].type_expr,
            IrTypeExpr::Primitive(IrPrimitive::Integer)
        );
    }
}

#[test]
fn opaque_rendering_does_not_erase_a_nested_unions_selection_policy() {
    use crate::ir::types::IrUnion;
    let mut ir = fixture(include_str!(
        "../../../../../tests/fixtures/valid/typescript-overlapping-allof.yaml"
    ));
    let mut opaque = ir.schemas["Left"].clone();
    opaque.name = "Opaque".into();
    opaque.kind = IrSchemaKind::Union(IrUnion {
        members: vec![IrTypeExpr::Any, IrTypeExpr::Named("LeftNode".into())],
        nullable: false,
    });
    ir.schemas.insert(opaque.name.clone(), opaque);
    let IrSchemaKind::Object(left) = &mut ir.schemas["Left"].kind else {
        panic!("object")
    };
    left.properties.get_mut("config").unwrap().type_expr = IrTypeExpr::Named("Opaque".into());
    assert_eq!(
        snapshot(&project(&ir).schemas["Overlap"]),
        snapshot(&ir.schemas["Overlap"])
    );
}

#[test]
fn container_only_alias_cycles_keep_the_fallback_without_expansion() {
    let ir = fixture(
        r"
openapi: 3.1.0
info: {title: alias cycles, version: '1'}
paths: {}
components:
  schemas:
    Left: {type: array, items: {$ref: '#/components/schemas/Left'}}
    Right: {type: array, items: {$ref: '#/components/schemas/Right'}}
    Overlap:
      allOf:
        - {type: object, properties: {config: {$ref: '#/components/schemas/Left'}}}
        - {type: object, properties: {config: {$ref: '#/components/schemas/Right'}}}
",
    );
    assert!(matches!(project(&ir), Cow::Borrowed(_)));
}

#[test]
fn recursive_projection_keeps_shared_models_and_authored_names_unchanged() {
    let mut ir = fixture(include_str!(
        "../../../../../tests/fixtures/valid/typescript-overlapping-allof.yaml"
    ));
    ir.schemas.retain(|name, _| {
        [
            "Left",
            "Right",
            "LeftNode",
            "RightNode",
            "Overlap",
            "OverlapConfig",
        ]
        .contains(&name.as_str())
    });
    let before = snapshot(&ir);
    let projected = project(&ir);
    assert!(matches!(projected, Cow::Owned(_)));
    assert_eq!(snapshot(&ir), before);
    for name in ["Left", "Right", "LeftNode", "RightNode", "OverlapConfig"] {
        assert_eq!(
            snapshot(&projected.schemas[name]),
            snapshot(&ir.schemas[name])
        );
    }
    let IrSchemaKind::Object(overlap) = &projected.schemas["Overlap"].kind else {
        panic!("overlap must have one object view");
    };
    assert!(!overlap.properties["config"].required);
    let IrTypeExpr::Named(name) = &overlap.properties["config"].type_expr else {
        panic!("expected named nested view");
    };
    assert_eq!(name, "OverlapConfig2");
    let IrSchemaKind::Object(node) = &projected.schemas[name].kind else {
        panic!("expected object view");
    };
    assert!(node.properties["first_name"].required);
    assert!(node.properties["last_name"].required);
    assert_eq!(node.properties["json_schema"].type_expr, IrTypeExpr::Any);
    assert_eq!(
        node.properties["children"].type_expr,
        IrTypeExpr::Array(Box::new(IrTypeExpr::Named(name.clone())))
    );
    assert_eq!(
        node.properties["by_key"].type_expr,
        IrTypeExpr::Map(Box::new(IrTypeExpr::Named(name.clone())))
    );
}

#[test]
fn incompatible_overlap_rolls_back_helpers_and_does_not_change_fallback() {
    let ir = fixture(
        r"
openapi: 3.1.0
info: {title: rollback, version: '1'}
paths: {}
components:
  schemas:
    Left:
      type: object
      properties:
        config: {type: object, properties: {first_name: {type: string}}}
        conflict: {type: string}
    Right:
      type: object
      properties:
        config: {type: object, properties: {last_name: {type: string}}}
        conflict: {type: integer}
    Overlap:
      allOf:
        - {$ref: '#/components/schemas/Left'}
        - {$ref: '#/components/schemas/Right'}
",
    );
    assert!(matches!(project(&ir), Cow::Borrowed(_)));
    let mut projection = ir.clone();
    // A later successful view must not retain a failed view's reserved helper.
    let mut left = projection.schemas["Left"].clone();
    let mut right = projection.schemas["Right"].clone();
    for schema in [&mut left, &mut right] {
        let IrSchemaKind::Object(object) = &mut schema.kind else {
            panic!("object")
        };
        object.properties.shift_remove("conflict");
    }
    left.name = "CompatibleLeft".into();
    right.name = "CompatibleRight".into();
    projection.schemas.insert(left.name.clone(), left);
    projection.schemas.insert(right.name.clone(), right);
    let mut compatible = projection.schemas["Overlap"].clone();
    compatible.name = "Compatible".into();
    let IrSchemaKind::Intersection(all) = &mut compatible.kind else {
        panic!("intersection")
    };
    all.members = vec![
        IrTypeExpr::Named("CompatibleLeft".into()),
        IrTypeExpr::Named("CompatibleRight".into()),
    ];
    projection
        .schemas
        .insert(compatible.name.clone(), compatible);
    let projected = project(&projection);
    assert_eq!(
        snapshot(&projected.schemas["Overlap"]),
        snapshot(&ir.schemas["Overlap"])
    );
    assert!(!projected.schemas.contains_key("OverlapConfig"));
    assert!(projected.schemas.contains_key("CompatibleConfig"));
}
