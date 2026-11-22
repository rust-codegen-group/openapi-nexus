use openapi_nexus::IrSpec;
use openapi_nexus::generators::union_wire::{Selection, SelectionError, Strategy, UnionWirePlans};
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;
use serde_json::{Value, json};

fn fixture() -> IrSpec {
    let source = include_str!("fixtures/valid/inferred-union-selection.yaml");
    lower(parse_content_yaml(source).expect("parse inferred union fixture"))
        .expect("lower inferred union fixture")
}

#[test]
fn absent_discriminator_uses_finite_payload_values() {
    let ir = fixture();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let choice = plans.get("Choice").unwrap();

    for (value, index) in [
        (json!({"label": "alpha"}), 0),
        (json!({"label": "beta"}), 1),
    ] {
        assert_eq!(
            choice.select(&value),
            Ok(Selection::Branch(index)),
            "{value}"
        );
    }
    let value = json!({"label": "unknown"});
    assert_eq!(choice.select(&value), Err(SelectionError::NoMatch));
}

#[test]
fn selection_strategies_count_authored_branches_and_preserve_raw_input() {
    let ir = fixture();
    let first = UnionWirePlans::build(&ir, Strategy::FirstMatch);
    let strict = UnionWirePlans::build(&ir, Strategy::Strict);
    let raw = UnionWirePlans::build(&ir, Strategy::Raw);
    let first = first.get("Choice").unwrap();
    let strict = strict.get("Choice").unwrap();
    let raw = raw.get("Choice").unwrap();

    let overlap = json!({"label": "shared"});
    assert_eq!(first.select(&overlap), Ok(Selection::Branch(0)));
    assert_eq!(
        strict.select(&overlap),
        Err(SelectionError::Ambiguous(vec![0, 1]))
    );

    let explicit = json!({"kind": "b_alt", "label": "shared"});
    assert_eq!(first.select(&explicit), Ok(Selection::Branch(1)));
    assert_eq!(strict.select(&explicit), Ok(Selection::Branch(1)));

    let incompatible = json!({"kind": "a", "label": "beta"});
    assert_eq!(first.select(&incompatible), Err(SelectionError::NoMatch));
    assert_eq!(strict.select(&incompatible), Err(SelectionError::NoMatch));

    for value in [overlap, explicit, incompatible, Value::Null] {
        let Selection::Raw(original) = raw.select(&value).unwrap() else {
            panic!("raw should not select a branch for {value}");
        };
        assert!(std::ptr::eq(original, &value));
    }
}

#[test]
fn required_nullable_discriminator_is_distinct_from_absence() {
    let ir = fixture();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let choice = plans.get("Choice").unwrap();

    let explicit_null = json!({"kind": null, "label": "gamma"});
    let absent = json!({"label": "gamma"});
    let null_in_non_nullable_variant = json!({"kind": null, "label": "alpha"});
    assert_eq!(choice.select(&explicit_null), Ok(Selection::Branch(2)));
    assert_eq!(choice.select(&absent), Err(SelectionError::NoMatch));
    assert_eq!(
        choice.select(&null_in_non_nullable_variant),
        Err(SelectionError::NoMatch)
    );
}

#[test]
fn nested_named_union_checks_recursive_dictionary_values() {
    let ir = fixture();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let tree = plans.get("Tree").unwrap();
    let envelope = plans.get("Envelope").unwrap();

    for (value, leaf_selection, tree_selection) in [
        (
            json!({"kind": "leaf", "label": "end", "count": 2}),
            Ok(Selection::Branch(1)),
            Ok(Selection::Branch(0)),
        ),
        (
            json!({"kind": "leaf", "label": "end", "count": "not an integer"}),
            Err(SelectionError::NoMatch),
            Err(SelectionError::NoMatch),
        ),
    ] {
        assert_eq!(tree.select(&value), leaf_selection, "leaf: {value}");

        let root = json!({"kind": "node", "children": {"a": {"children": {"b": value}}}});
        assert_eq!(tree.select(&root), tree_selection, "tree: {root}");

        let nested = json!({"kind": "tree", "tree": root});
        assert_eq!(
            envelope.select(&nested),
            tree_selection,
            "envelope: {nested}"
        );
    }
}

#[test]
fn all_of_does_not_extend_a_conjuncts_known_property_names() {
    let ir = fixture();
    let plans = UnionWirePlans::build(&ir, Strategy::Strict);
    let composed = plans.get("Composed").unwrap();

    let absent = json!({"label": "a", "count_extra": 3});
    let explicit = json!({"kind": "a", "label": "a", "count_extra": 3});
    assert_eq!(composed.select(&absent), Ok(Selection::Branch(0)));
    assert_eq!(composed.select(&explicit), Err(SelectionError::NoMatch));
}
