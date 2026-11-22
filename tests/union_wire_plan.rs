use openapi_nexus::generators::union_wire::{Selection, SelectionError, Strategy, UnionWirePlans};
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;
use serde_json::json;

#[test]
fn authored_labels_and_absence_select_the_same_branches_in_each_version() {
    let source = include_str!("fixtures/valid/optional-discriminator-union.yaml");
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("Union").unwrap();
        for (index, allows_absent, values) in [
            (0, true, vec![json!("variant_a")]),
            (1, false, vec![json!("variant_b_1"), json!("variant_b_2")]),
            (2, false, vec![json!("variant_c")]),
            (3, true, vec![json!("variant_d")]),
        ] {
            let domain = plan.tag_domain(index).unwrap();
            assert_eq!(domain.field, "kind");
            assert_eq!(domain.allows_absent, allows_absent, "{version}: {index}");
            assert_eq!(domain.values, Some(values), "{version}: {index}");
        }
        for (wire, index) in [
            (json!({"payload_a": "a", "payload_a_schema": {}}), 0),
            (
                json!({"kind": "variant_a", "payload_a": "a", "payload_a_schema": {}}),
                0,
            ),
            (json!({"kind": "variant_b_1", "payload_b": 3}), 1),
            (json!({"kind": "variant_b_2", "payload_b": 3}), 1),
            (json!({"kind": "variant_c", "payload_c": "c"}), 2),
        ] {
            assert_eq!(
                plan.select(&wire),
                Ok(Selection::Branch(index)),
                "{version}: {wire}"
            );
        }
        for wire in [
            json!({"kind": null, "payload_a": "a", "payload_a_schema": {}}),
            json!({"kind": "unknown", "payload_a": "a", "payload_a_schema": {}}),
            json!({"kind": "PayloadB", "payload_b": 3}),
            json!({"kind": "variant_b_2", "payload_a": "a", "payload_a_schema": {}}),
        ] {
            assert_eq!(
                plan.select(&wire),
                Err(SelectionError::NoMatch),
                "{version}: {wire}"
            );
        }
    }
}

#[test]
fn read_and_write_use_one_owner_for_a_tag_retained_by_an_allof_payload() {
    let source = r##"
openapi: 3.1.0
info: {title: Composed tags, version: '1'}
paths: {}
components:
  schemas:
    Base:
      type: object
      required: [kind, label]
      properties:
        kind: {type: string, enum: [variant_a, variant_c]}
        label: {type: string}
    Extra:
      type: object
      required: [count]
      properties:
        count: {type: integer}
    Choice:
      oneOf:
        - allOf:
            - {$ref: '#/components/schemas/Base'}
            - {$ref: '#/components/schemas/Extra'}
            - properties:
                kind: {type: string, enum: [variant_a]}
        - allOf:
            - {$ref: '#/components/schemas/Extra'}
            - properties:
                kind: {type: string, enum: [variant_b]}
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let plan = plans.get("Choice").unwrap();
    for (index, wire, projected) in [
        (
            0,
            json!({"kind": "variant_a", "label": "a", "count": 1}),
            json!({"kind": "variant_a", "label": "a", "count": 1}),
        ),
        (
            1,
            json!({"kind": "variant_b", "count": 2}),
            json!({"count": 2}),
        ),
        (1, json!({"count": 2}), json!({"count": 2})),
    ] {
        assert_eq!(plan.select(&wire), Ok(Selection::Branch(index)));
        let binding = plan.binding(index).unwrap();
        assert_eq!(*binding.payload(&wire).unwrap(), projected);
        let mut object = projected.as_object().unwrap().clone();
        binding.write_tag(&mut object, binding.tag(&wire)).unwrap();
        assert_eq!(serde_json::Value::Object(object), wire);
    }
    let mut object = json!({"kind": "variant_c", "label": "a", "count": 1})
        .as_object()
        .unwrap()
        .clone();
    let claimed = json!("variant_a");
    assert!(
        plan.binding(0)
            .unwrap()
            .write_tag(&mut object, Some(&claimed))
            .is_err()
    );
    assert_eq!(object["kind"], "variant_c");
}

#[test]
fn matching_and_projection_keep_external_and_adjacent_envelopes_distinct() {
    let source = r##"
openapi: 3.1.0
info: {title: Envelope shapes, version: '1'}
paths: {}
components:
  schemas:
    Item:
      type: object
      required: [label]
      properties:
        label: {type: string}
    ExternalChoice:
      oneOf:
        - type: object
          required: [left]
          properties:
            left: {$ref: '#/components/schemas/Item'}
        - type: object
          required: [right]
          properties:
            right: {$ref: '#/components/schemas/Item'}
    AdjacentChoice:
      oneOf:
        - type: object
          required: [payload]
          properties:
            kind: {type: string, enum: [left, alternate]}
            payload: {$ref: '#/components/schemas/Item'}
        - type: object
          required: [kind, payload]
          properties:
            kind: {type: string, enum: [right]}
            payload: {$ref: '#/components/schemas/Item'}
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let payload = json!({"label": "value"});
    let external = plans.get("ExternalChoice").unwrap();
    let input = json!({"right": payload});
    assert_eq!(external.select(&input), Ok(Selection::Branch(1)));
    let binding = external.binding(1).unwrap();
    assert_eq!(*binding.payload(&input).unwrap(), payload);
    assert_eq!(binding.tag(&input), None);
    assert!(external.tag_domain(1).is_none());
    assert_eq!(
        external.select(&json!({"right": null})),
        Err(SelectionError::NoMatch)
    );
    let adjacent = plans.get("AdjacentChoice").unwrap();
    let input = json!({"kind": "alternate", "payload": payload});
    assert_eq!(adjacent.select(&input), Ok(Selection::Branch(0)));
    let binding = adjacent.binding(0).unwrap();
    assert_eq!(*binding.payload(&input).unwrap(), payload);
    let mut envelope = serde_json::Map::from_iter([("payload".to_owned(), payload)]);
    binding
        .write_tag(&mut envelope, binding.tag(&input))
        .unwrap();
    assert_eq!(serde_json::Value::Object(envelope), input);
    let domain = adjacent.tag_domain(0).unwrap();
    assert!(domain.allows_absent);
    assert_eq!(domain.values, Some(vec![json!("left"), json!("alternate")]));
}

#[test]
fn a_referenced_null_constant_is_not_an_unconstrained_tag() {
    let source = r##"
openapi: 3.1.0
info: {title: Nullable tag, version: '1'}
paths: {}
components:
  schemas:
    Payload:
      type: object
      required: [kind, label]
      properties:
        kind: {const: null}
        label: {type: string}
    Choice:
      oneOf:
        - allOf:
            - type: object
              properties:
                kind: {type: string, enum: [b]}
                count: {type: integer}
        - {$ref: '#/components/schemas/Payload'}
"##;
    for version in ["3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("Choice").unwrap();
        assert_eq!(
            plan.select(&json!({"kind": null, "label": "x"})),
            Ok(Selection::Branch(1))
        );
        assert_eq!(
            plan.select(&json!({"kind": "wrong", "label": "x"})),
            Err(SelectionError::NoMatch)
        );
    }
}

#[test]
fn finite_numeric_guards_compare_integers_without_rounding() {
    let source = r#"
openapi: 3.1.0
info: {title: Finite number selection, version: '1'}
paths: {}
components:
  schemas:
    Choice:
      anyOf:
        - {type: integer, enum: [1, 9007199254740993, 18446744073709551615]}
        - {type: number, enum: [9007199254740992.0, -2.0]}
        - {type: boolean, enum: [true]}
"#;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let plan = plans.get("Choice").unwrap();
    for (wire, index) in [
        (json!(1.0), 0),
        (json!(9007199254740993_u64), 0),
        (json!(u64::MAX), 0),
        (json!(9007199254740992_u64), 1),
        (json!(-2), 1),
        (json!(true), 2),
    ] {
        assert_eq!(plan.select(&wire), Ok(Selection::Branch(index)), "{wire}");
    }
    assert_eq!(
        plan.select(&json!(18446744073709551616.0)),
        Err(SelectionError::NoMatch)
    );
    assert_eq!(plan.select(&json!(false)), Err(SelectionError::NoMatch));
}

#[test]
fn union_level_constraints_apply_before_branch_selection() {
    let source = include_str!("fixtures/valid/optional-discriminator-union.yaml").replace(
        "    Union:\n      oneOf:",
        "    Union:\n      required: [scope]\n      oneOf:",
    );
    let ir = lower(parse_content_yaml(&source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let invalid = json!({"kind": "variant_b_2", "payload_b": 3});
    let valid = json!({"kind": "variant_b_2", "payload_b": 3, "scope": "x"});
    for (site, index) in [("Union", 1), ("UnionContainer", 0)] {
        let plan = plans.get(site).unwrap();
        assert_eq!(
            plan.select(&invalid),
            Err(SelectionError::NoMatch),
            "{site}"
        );
        assert_eq!(plan.select(&valid), Ok(Selection::Branch(index)), "{site}");
    }
}

#[test]
fn intersecting_optional_labels_can_leave_only_the_absent_state() {
    let source = r##"
openapi: 3.1.0
info: {title: Inferred tags, version: '1'}
paths: {}
components:
  schemas:
    Choice:
      oneOf:
        - allOf:
            - type: object
              required: [label]
              properties:
                kind: {type: string, enum: [a]}
                label: {type: string}
            - type: object
              properties:
                kind: {type: string, enum: [b]}
        - allOf:
            - type: object
              required: [kind, amount]
              properties:
                kind: {type: string, enum: [c]}
                amount: {type: integer}
"##;
    for version in ["3.0.3", "3.1.0", "3.2.0"] {
        let ir = lower(parse_content_yaml(&source.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("Choice").unwrap();
        assert_eq!(
            plan.select(&json!({"label": "x"})),
            Ok(Selection::Branch(0))
        );
        assert_eq!(
            plan.select(&json!({"kind": "a", "label": "x"})),
            Err(SelectionError::NoMatch)
        );
        assert_eq!(
            plan.select(&json!({"kind": "b", "label": "x"})),
            Err(SelectionError::NoMatch)
        );
    }
}
