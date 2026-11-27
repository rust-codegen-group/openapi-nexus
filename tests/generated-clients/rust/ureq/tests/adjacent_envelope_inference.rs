use rust_adjacent_envelopes::models::{
    AdjacentWithUnit, MixedEnvelopes, MixedEnvelopesSameKey, OptionalContent,
};
use serde_json::{from_value, json, to_value};

#[test]
fn optional_content_can_be_omitted_or_present() {
    for wire in [
        json!({"kind":"text"}),
        json!({"kind":"text","payload":"value"}),
        json!({"kind":"number"}),
        json!({"kind":"number","payload":2}),
    ] {
        let value: OptionalContent = from_value(wire.clone()).unwrap();
        assert_eq!(to_value(value).unwrap(), wire);
    }
    for wire in [
        json!({}),
        json!({"kind":"other"}),
        json!({"kind":"text","payload":2}),
    ] {
        assert!(
            from_value::<OptionalContent>(wire.clone()).is_err(),
            "{wire}"
        );
    }
}

#[test]
fn mixed_wrappers_keep_their_original_fields() {
    let legacy = json!({"legacy":{"size":2}});
    let adjacent = json!({"kind":"new","payload":{"size":3}});
    for wire in [legacy, adjacent.clone()] {
        let value: MixedEnvelopes = from_value(wire.clone()).unwrap();
        assert_eq!(to_value(value).unwrap(), wire);
    }
    for wire in [json!({"kind":{"size":2}}), adjacent] {
        let value: MixedEnvelopesSameKey = from_value(wire.clone()).unwrap();
        assert_eq!(to_value(value).unwrap(), wire);
    }
}

#[test]
fn a_true_adjacent_payload_and_its_tag_only_sibling_still_round_trip() {
    for wire in [
        json!({"kind":"empty"}),
        json!({"kind":"new","payload":{"size":3}}),
    ] {
        let value: AdjacentWithUnit = from_value(wire.clone()).unwrap();
        assert_eq!(to_value(value).unwrap(), wire);
    }
    assert!(from_value::<AdjacentWithUnit>(json!({"kind":"new"})).is_err());
}
