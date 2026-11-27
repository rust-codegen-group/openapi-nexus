//! Exercise reversible envelope classification through the public lowering interface.

use openapi_nexus::generators::union_wire::{
    Selection, SelectionError, Strategy, UnionWirePlans, WireBinding,
};
use openapi_nexus::ir::lower::lower;
use openapi_nexus::parser::parse_content_yaml;
use serde_json::json;

const FIXTURE: &str = include_str!("fixtures/valid/adjacent-envelope-presence.yaml");
const VERSIONS: &[&str] = &["3.0.3", "3.1.0", "3.2.0"];

#[test]
fn optional_adjacent_content_is_selected_and_decoded_as_a_whole_object() {
    for version in VERSIONS {
        let ir =
            lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("OptionalContent").unwrap();
        for (branch, wire) in [
            (0, json!({"kind":"text"})),
            (0, json!({"kind":"text","payload":"value"})),
            (1, json!({"kind":"number"})),
            (1, json!({"kind":"number","payload":2})),
        ] {
            assert_eq!(
                plan.select(&wire),
                Ok(Selection::Branch(branch)),
                "{version}"
            );
            let binding = plan.binding(branch).unwrap();
            assert_eq!(binding, WireBinding::Whole, "{version}");
            assert_eq!(*binding.payload(&wire).unwrap(), wire, "{version}");
        }
        assert_eq!(
            plan.select(&json!({"kind":"text","payload":2})),
            Err(SelectionError::NoMatch),
            "{version}"
        );
    }
}

#[test]
fn unrelated_wrappers_do_not_inherit_a_siblings_discriminator() {
    for version in VERSIONS {
        let ir =
            lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        for (name, legacy) in [
            ("MixedEnvelopes", json!({"legacy":{"size":2}})),
            ("MixedEnvelopesSameKey", json!({"kind":{"size":2}})),
        ] {
            let plan = plans.get(name).unwrap();
            for (branch, wire) in [legacy, json!({"kind":"new","payload":{"size":3}})]
                .into_iter()
                .enumerate()
            {
                assert_eq!(
                    plan.select(&wire),
                    Ok(Selection::Branch(branch)),
                    "{version} {name}"
                );
                let binding = plan.binding(branch).unwrap();
                assert_eq!(binding, WireBinding::Whole, "{version} {name}");
                assert_eq!(*binding.payload(&wire).unwrap(), wire, "{version} {name}");
            }
        }
    }
}

#[test]
fn a_proven_tag_only_member_can_still_share_an_adjacent_envelope() {
    for version in VERSIONS {
        let ir =
            lower(parse_content_yaml(&FIXTURE.replacen("3.1.0", version, 1)).unwrap()).unwrap();
        let plans = UnionWirePlans::build(&ir, Strategy::Strict);
        let plan = plans.get("AdjacentWithUnit").unwrap();
        assert_eq!(
            plan.select(&json!({"kind":"empty"})),
            Ok(Selection::Branch(0)),
            "{version}"
        );
        assert_eq!(
            plan.select(&json!({"kind":"new","payload":{"size":3}})),
            Ok(Selection::Branch(1)),
            "{version}"
        );
        assert_eq!(
            plan.select(&json!({"kind":"new"})),
            Err(SelectionError::NoMatch),
            "{version}"
        );
        assert_eq!(
            plan.binding(1),
            Some(WireBinding::Adjacent {
                field: "kind",
                content: "payload"
            }),
            "{version}"
        );
    }
}
