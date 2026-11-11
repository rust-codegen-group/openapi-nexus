use serde_json::{Value, json};

fn finite_wire() -> Value {
    json!({"revision": 2, "values": [2, 2.5], "flags": {"keep_key": true}, "second_value": "two"})
}

// These are ordinary in-process tests compiled once against each independently
// generated policy crate. No generation or runner orchestration happens here.
macro_rules! typed_cases {
    ($module:ident, $client:ident) => {
        mod $module {
            use super::*;
            use $client::models::*;

            #[test]
            fn finite_scalar_array_map_facts_select_and_preserve_the_member() {
                let value: FiniteChoice = serde_json::from_value(finite_wire()).unwrap();
                assert!(matches!(value, FiniteChoice::FiniteTwo(_)));
                let expected = json!({"revision": 2, "values": [2.0, 2.5], "flags": {"keep_key": true}, "second_value": "two"});
                assert_eq!(serde_json::to_value(value).unwrap(), expected);
                for invalid in [
                    json!({"revision": 3, "values": [2.5], "flags": {"key": true}}),
                    json!({"revision": 2, "values": [1.5], "flags": {"key": true}}),
                    json!({"revision": 2, "values": [2.5], "flags": {"key": false}}),
                    json!({"revision": true, "values": [2.5], "flags": {"key": true}}),
                ] {
                    assert!(serde_json::from_value::<FiniteChoice>(invalid).is_err());
                }
            }

            #[test]
            fn container_constraints_distinguish_members_without_a_scalar_tag() {
                let wire = json!({"values": [2, 2], "second_value": "two"});
                let value: ContainerChoice = serde_json::from_value(wire.clone()).unwrap();
                assert!(matches!(value, ContainerChoice::ContainerTwo(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
                let wire = json!({"keep_key": 2});
                let value: MapChoice = serde_json::from_value(wire.clone()).unwrap();
                assert!(matches!(value, MapChoice::MapTwo(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
            }

            #[test]
            fn integers_above_double_precision_do_not_collapse_enum_members() {
                for revision in [9007199254740993_u64, u64::MAX] {
                    let wire = json!({"revision": revision, "second_value": "two"});
                    let value: LargeChoice = serde_json::from_value(wire.clone()).unwrap();
                    assert!(matches!(value, LargeChoice::LargeTwo(_)));
                    assert_eq!(serde_json::to_value(value).unwrap(), wire);
                }
            }

            #[test]
            fn scalar_unions_keep_numeric_boolean_and_finite_boundaries() {
                assert!(matches!(serde_json::from_value::<ScalarChoice>(json!(1)).unwrap(), ScalarChoice::Integer(1)));
                assert!(matches!(serde_json::from_value::<ScalarChoice>(json!(2.5)).unwrap(), ScalarChoice::Number(value) if value == 2.5));
                assert!(matches!(serde_json::from_value::<ScalarChoice>(json!(true)).unwrap(), ScalarChoice::Boolean(true)));
                for invalid in [json!(2), json!(false), json!("1"), Value::Null] {
                    assert!(serde_json::from_value::<ScalarChoice>(invalid).is_err());
                }
            }

            #[test]
            fn finite_wrapper_null_permissions_are_not_widened() {
                let value: NullableFields = serde_json::from_value(json!({"revision": null, "second_value": "two"})).unwrap();
                assert!(matches!(value, NullableFields::IncludedNull(_)));
                let value: NullableFields = serde_json::from_value(json!({"revision": 1})).unwrap();
                assert!(matches!(value, NullableFields::ExcludedNull(_)));
            }

            #[test]
            fn nullable_use_sites_do_not_add_a_public_enum_variant() {
                let envelope: NullableEnvelope = serde_json::from_value(json!({"choice": null})).unwrap();
                assert!(envelope.choice.is_none());
                // The typed enum itself has no synthetic Null/Raw variant.
                assert!(serde_json::from_value::<NullableChoice>(Value::Null).is_err());
                assert!(serde_json::from_value::<Option<NullableChoice>>(Value::Null).unwrap().is_none());
            }

            #[test]
            fn opaque_fallbacks_do_not_hide_concrete_members_or_map_values() {
                let value: OpaqueFirst = serde_json::from_value(json!({"wire_name": "detail"})).unwrap();
                assert!(matches!(value, OpaqueFirst::Alpha(_)));
                let wire = json!({"keep_key": {"wire_name": "detail"}, "opaque": 42});
                let value: MapOuter = serde_json::from_value(wire.clone()).unwrap();
                let MapOuter::OpenMap(map) = &value else { panic!("concrete map wins"); };
                assert!(matches!(map.additional_properties["keep_key"], OpaqueFirst::Alpha(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
                let value: OpaqueFirst = serde_json::from_value(json!(42)).unwrap();
                assert_eq!(serde_json::to_value(value).unwrap(), json!(42));
            }

            #[test]
            fn recursive_selection_checks_deep_members_and_preserves_payloads() {
                let wire = json!({"revision": 1, "children": [{"revision": 2, "children": [], "second_value": "leaf"}]});
                let value: Tree = serde_json::from_value(wire.clone()).unwrap();
                let Tree::BranchOne(branch) = &value else { panic!("root one"); };
                assert!(matches!(branch.children[0], Tree::BranchTwo(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
                assert!(serde_json::from_value::<Tree>(json!({"revision": 1, "children": [{"revision": 3, "children": []}]})).is_err());
            }

            #[test]
            fn deep_recursive_selection_preserves_payloads_and_rejects_unmatched_leaves() {
                // Both members inspect the same children before their disjoint
                // revision field. Reuse recursive matches, including failures.
                let mut wire = json!({"revision": 2, "children": [], "second_value": "leaf"});
                let mut invalid = json!({"revision": 3, "children": []});
                for _ in 0..24 {
                    wire = json!({"revision": 2, "children": [wire], "second_value": "branch"});
                    invalid = json!({"revision": 2, "children": [invalid]});
                }
                let value: Tree = serde_json::from_value(wire.clone()).unwrap();
                assert!(matches!(value, Tree::BranchTwo(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
                assert!(serde_json::from_value::<Tree>(invalid).is_err());
            }

            #[test]
            fn recursive_open_members_retain_both_concrete_and_raw_paths() {
                let wire = json!({"keep_key": {"wire_name": "root", "children": [42, {"wire_name": "leaf", "children": []}]}, "opaque": false});
                let value: RecursiveMapOuter = serde_json::from_value(wire.clone()).unwrap();
                let RecursiveMapOuter::OpenTreeMap(map) = &value else { panic!("concrete map wins"); };
                let OpenTree::OpenBranch(branch) = &map.additional_properties["keep_key"] else { panic!("concrete root"); };
                assert!(matches!(branch.children[1], OpenTree::OpenBranch(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
                let wire = json!({"opaque": false});
                let value: RecursiveMapOuter = serde_json::from_value(wire.clone()).unwrap();
                assert!(matches!(value, RecursiveMapOuter::Variant0(_)));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
            }

            #[test]
            fn binary_members_match_their_native_byte_array_wire_representation() {
                assert!(matches!(serde_json::from_value::<BinaryChoice>(json!([0, 255])).unwrap(), BinaryChoice::Binary(_)));
                assert!(matches!(serde_json::from_value::<BinaryChoice>(json!("text")).unwrap(), BinaryChoice::String(_)));
                assert!(serde_json::from_value::<BinaryChoice>(json!([256])).is_err());
            }

            #[test]
            fn extra_derives_and_utoipa_work_with_manual_deserialization() {
                let first: ScalarChoice = serde_json::from_value(json!(true)).unwrap();
                let second = first.clone();
                assert_eq!(first, second);
                assert_eq!(<FiniteChoice as utoipa::ToSchema>::name(), "FiniteChoice");
                let schema = serde_json::to_value(<FiniteChoice as utoipa::PartialSchema>::schema()).unwrap();
                assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
            }

            #[test]
            fn optional_opaque_properties_keep_object_shape_checks_warning_clean() {
                let value: OptionalOpaqueChoice = serde_json::from_value(json!({})).unwrap();
                assert!(matches!(value, OptionalOpaqueChoice::OptionalOpaque(_)));
                let value: OptionalOpaqueChoice = serde_json::from_value(json!("opaque")).unwrap();
                assert!(matches!(value, OptionalOpaqueChoice::String(_)));
                assert!(serde_json::from_value::<OptionalOpaqueChoice>(json!([42])).is_err());
            }
        }
    };
}

typed_cases!(first_match, rust_union_first_match);
typed_cases!(strict, rust_union_strict);

#[test]
fn first_match_keeps_schema_order_for_ambiguous_concrete_members() {
    use rust_union_first_match::models::{Ambiguous, OpenNested};
    let wire = json!({"wire_name": "detail"});
    let value: Ambiguous = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(value, Ambiguous::Alpha(_)));
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
    assert!(serde_json::from_value::<OpenNested>(wire).is_ok());
}

#[test]
fn strict_ambiguity_and_nested_delegate_errors_do_not_fall_back_to_raw() {
    use rust_union_strict::models::{Ambiguous, OpenNested};
    let wire = json!({"wire_name": "detail"});
    for error in [
        serde_json::from_value::<Ambiguous>(wire.clone()).unwrap_err(),
        serde_json::from_value::<OpenNested>(wire).unwrap_err(),
    ] {
        assert!(
            error
                .to_string()
                .contains("ambiguous untagged union Ambiguous"),
            "{error}"
        );
    }
}

#[test]
fn raw_keeps_known_ambiguous_unmatched_and_null_values_without_typed_dispatch() {
    use rust_union_raw::models::{Ambiguous, FiniteChoice, ScalarChoice};
    for wire in [
        finite_wire(),
        json!({"unmatched": [null, true]}),
        Value::Null,
    ] {
        let value: FiniteChoice = serde_json::from_value(wire.clone()).unwrap();
        let FiniteChoice::Raw(raw) = &value;
        assert_eq!(raw, &wire);
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
    let wire = json!({"wire_name": "detail", "second_value": "untouched"});
    let value: Ambiguous = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
    let value: ScalarChoice = serde_json::from_value(json!({"raw": 42})).unwrap();
    assert_eq!(value, value.clone());
    let schema = serde_json::to_value(<FiniteChoice as utoipa::PartialSchema>::schema()).unwrap();
    assert_eq!(schema["oneOf"].as_array().unwrap().len(), 1);
}
