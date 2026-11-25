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
fn composed_payload_readers_keep_the_tag_but_cannot_overwrite_it() {
    let source = include_str!("fixtures/valid/rust-inferred-tagged-wire.yaml");
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let plan = plans.get("ComposedTagged").unwrap();
    let wire = json!({"kind": "alpha_alt", "label": "a", "count": 3});
    assert_eq!(plan.select(&wire), Ok(Selection::Branch(1)));
    let binding = plan.binding(1).unwrap();
    assert_eq!(*binding.payload(&wire).unwrap(), wire);

    let mut output = wire.as_object().unwrap().clone();
    binding.write_tag(&mut output, binding.tag(&wire)).unwrap();
    output.insert("kind".into(), json!("alpha"));
    assert!(binding.write_tag(&mut output, binding.tag(&wire)).is_err());
    assert_eq!(output["kind"], "alpha");
}

#[test]
fn a_payload_union_can_have_tag_readers_and_tagless_alternatives() {
    let source = r##"
openapi: 3.1.0
info: {title: Conditional tag ownership, version: '1'}
paths: {}
components:
  schemas:
    WithTag:
      type: object
      required: [kind, label]
      properties:
        kind: {type: string, enum: [a]}
        label: {type: string}
    WithoutTag:
      type: object
      required: [count]
      properties:
        count: {type: integer}
    Payload:
      anyOf:
        - {$ref: '#/components/schemas/WithTag'}
        - {$ref: '#/components/schemas/WithoutTag'}
    Choice:
      oneOf:
        - allOf:
            - {$ref: '#/components/schemas/Payload'}
            - properties:
                kind: {type: string, enum: [a, b]}
        - allOf:
            - properties:
                kind: {type: string, enum: [c]}
                text: {type: string}
              required: [kind, text]
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let choice = plans.get("Choice").unwrap();
    let payload_plan = plans.get("Payload").unwrap();
    let binding = choice.binding(0).unwrap();
    for (wire, index, encoded_payload) in [
        (
            json!({"kind": "a", "label": "x"}),
            0,
            json!({"kind": "a", "label": "x"}),
        ),
        (json!({"kind": "b", "count": 2}), 1, json!({"count": 2})),
    ] {
        assert_eq!(choice.select(&wire), Ok(Selection::Branch(0)));
        let payload = binding.payload(&wire).unwrap();
        assert_eq!(payload_plan.select(&payload), Ok(Selection::Branch(index)));
        let mut output = encoded_payload.as_object().unwrap().clone();
        binding.write_tag(&mut output, binding.tag(&wire)).unwrap();
        assert_eq!(serde_json::Value::Object(output), wire);
    }
}

#[test]
fn fixed_tag_compatibility_requires_distinct_required_authored_labels() {
    let source = r##"
openapi: 3.1.0
info: {title: Fixed label identities, version: '1'}
paths: {}
components:
  schemas:
    Choice:
      oneOf:
        - allOf:
            - type: object
              required: [kind, left]
              properties:
                kind: {type: string, enum: [left]}
                left: {type: string}
        - allOf:
            - type: object
              required: [kind, right]
              properties:
                kind: {type: string, enum: [right]}
                right: {type: string}
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::FirstMatch);
    let fixed = plans.get("Choice").unwrap();
    assert!(fixed.has_distinct_fixed_tags());
    assert_eq!(
        fixed.select(&json!({"kind": "left"})),
        Err(SelectionError::NoMatch)
    );

    let repeated = source.replace("enum: [right]", "enum: [left]");
    let ir = lower(parse_content_yaml(&repeated).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::FirstMatch);
    assert!(!plans.get("Choice").unwrap().has_distinct_fixed_tags());
}

#[test]
fn named_untagged_union_keeps_its_own_alternatives_and_sibling_gate() {
    let source = r##"
openapi: 3.1.0
info: {title: Untagged choices, version: '1'}
paths: {}
components:
  schemas:
    Choice:
      type: string
      allOf:
        - oneOf:
            - enum: [left, right]
            - type: integer
      anyOf:
        - type: integer
        - enum: [left]
        - enum: [right]
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let choice = plans.get("Choice").expect("plan for the root anyOf");
    assert_eq!(choice.select(&json!("left")), Ok(Selection::Branch(1)));
    assert_eq!(choice.select(&json!("right")), Ok(Selection::Branch(2)));
    assert_eq!(choice.select(&json!(6)), Err(SelectionError::NoMatch));
}

#[test]
fn a_referenced_tagged_decoder_still_receives_its_discriminator() {
    let source = r##"
openapi: 3.1.0
info: {title: Nested tagged payload, version: '1'}
paths: {}
components:
  schemas:
    Inner:
      oneOf:
        - allOf:
            - type: object
              required: [kind, left]
              properties:
                kind: {type: string, enum: [a, alternate]}
                left: {type: string}
        - allOf:
            - type: object
              required: [kind, right]
              properties:
                kind: {type: string, enum: [b]}
                right: {type: string}
    Outer:
      oneOf:
        - allOf:
            - {$ref: '#/components/schemas/Inner'}
            - properties:
                kind: {type: string, enum: [a, alternate, b]}
        - allOf:
            - type: object
              required: [kind, value]
              properties:
                kind: {type: string, enum: [c]}
                value: {type: string}
"##;
    let ir = lower(parse_content_yaml(source).unwrap()).unwrap();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let inner = plans.get("Inner").unwrap();
    let outer = plans.get("Outer").unwrap();
    let wire = json!({"kind": "b", "right": "value"});
    assert_eq!(outer.select(&wire), Ok(Selection::Branch(0)));
    let binding = outer.binding(0).unwrap();
    let payload = binding.payload(&wire).unwrap();
    assert_eq!(inner.select(&payload), Ok(Selection::Branch(1)));
    let mut output = payload.as_object().unwrap().clone();
    binding.write_tag(&mut output, binding.tag(&wire)).unwrap();
    assert_eq!(serde_json::Value::Object(output), wire);
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
