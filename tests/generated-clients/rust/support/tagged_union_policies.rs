use serde_json::{Value, json};

macro_rules! typed_cases {
    ($module:ident, $client:ident) => {
        mod $module {
            use super::*;
            use $client::models::*;

            #[test]
            fn inferred_tagged_union_preserves_original_tag_spelling_and_presence() {
                for wire in [
                    json!({"label": "a"}),
                    json!({"type": "variant_a", "label": "a"}),
                    json!({"type": "variant_b_v1", "label": "b"}),
                    json!({"type": "variant_b_v2", "label": "b"}),
                ] {
                    let model: TaggedChoice = serde_json::from_value(wire.clone())
                        .unwrap_or_else(|error| panic!("{wire}: {error}"));
                    assert_eq!(serde_json::to_value(model).unwrap(), wire);
                }
            }

            #[test]
            fn tagged_selection_keeps_composed_owner_and_validates_local_dictionary_keys() {
                for wire in [
                    json!({"kind": "alpha", "label": "a", "count": 1}),
                    json!({"kind": "alpha_alt", "label": "a", "count": 1}),
                    json!({"kind": "beta", "label": "b", "count_extra": 2}),
                    json!({"kind": null, "label": "b", "count_extra": 2}),
                ] {
                    let composed: ComposedTagged = serde_json::from_value(wire.clone()).unwrap();
                    assert_eq!(serde_json::to_string(&composed).unwrap().matches("\"kind\"").count(), 1);
                    assert_eq!(serde_json::to_value(composed).unwrap(), wire);
                    let nested: NestedTagged = serde_json::from_value(wire.clone()).unwrap();
                    assert!(matches!(nested, NestedTagged::ComposedTagged(_)));
                    assert_eq!(serde_json::to_value(nested).unwrap(), wire);
                }
                for wire in [
                    json!({"label": "a", "count": 1}),
                    json!({"kind": "alpha_other", "label": "a", "count": 1}),
                    json!({"kind": "beta", "label": "b", "count_extra": "wrong"}),
                ] {
                    assert!(serde_json::from_value::<ComposedTagged>(wire.clone()).is_err(), "{wire}");
                    assert!(serde_json::from_value::<NestedTagged>(wire.clone()).is_err(), "{wire}");
                }
            }

            #[test]
            fn serialization_rejects_a_payload_owned_tag_outside_its_composed_domain() {
                let value = ComposedTagged::Alpha(ComposedTaggedAlpha {
                    tagged_base: TaggedBase { kind: "alpha_alt".into(), label: "value".into() },
                    tagged_extra: TaggedExtra { count: 1 },
                });
                let error = serde_json::to_value(value).unwrap_err();
                assert!(error.to_string().contains("does not match inferred tagged union branch Alpha"), "{error}");
            }

            #[test]
            fn promoted_inline_tagged_union_keeps_nested_wire_eligibility() {
                for wire in [json!({"label": "beta", "scope": "s"}), json!({"kind": "a", "label": "alpha", "scope": "s"})] {
                    let model: InlineTagged = serde_json::from_value(wire.clone()).unwrap();
                    assert_eq!(serde_json::to_value(model).unwrap(), wire);
                }
                for invalid in [json!({"kind": "a", "label": "beta", "scope": "s"}), json!({"label": "beta"})] {
                    assert!(serde_json::from_value::<InlineTagged>(invalid).is_err());
                }
            }

            #[test]
            fn absent_tag_selects_using_finite_payload_constraints() {
                let wire = json!({"label": "b"});
                let value: AmbiguousTagged = serde_json::from_value(wire.clone()).unwrap();
                assert!(matches!(value, AmbiguousTagged::Beta { .. }));
                assert_eq!(serde_json::to_value(value).unwrap(), wire);
            }

        }
    };
}

typed_cases!(first_match, rust_tagged_first_match);
typed_cases!(strict, rust_tagged_strict);

#[test]
fn tagged_union_strategy_is_independent_and_preserves_a_raw_escape_hatch() {
    let wire = json!({"label": "both"});
    let first: rust_tagged_first_match::models::AmbiguousTagged =
        serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(
        first,
        rust_tagged_first_match::models::AmbiguousTagged::Alpha { .. }
    ));
    assert_eq!(serde_json::to_value(first).unwrap(), wire);
    let error = serde_json::from_value::<rust_tagged_strict::models::AmbiguousTagged>(wire.clone())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous inferred tagged union"),
        "{error}"
    );
    for wire in [wire, json!({"kind": "unknown"}), Value::Null] {
        let value: rust_tagged_raw::models::AmbiguousTagged =
            serde_json::from_value(wire.clone()).unwrap();
        let rust_tagged_raw::models::AmbiguousTagged::Raw(raw) = &value;
        assert_eq!(raw, &wire);
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
}
