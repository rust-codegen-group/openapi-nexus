use serde_json::json;
use single_field_tags_contract::models::{
    DistinctWrappers, FormatChoice, IdentifierChoice, ReferencedValues, SharedWrapper,
};

#[test]
fn discriminator_values_not_field_names_select_unit_cases() {
    for (label, variant) in [
        ("plain", FormatChoice::Plain),
        ("structured", FormatChoice::Structured),
    ] {
        let wire = json!({"kind": label});
        assert_eq!(serde_json::to_value(&variant).unwrap(), wire);
        let decoded: FormatChoice = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            std::mem::discriminant(&decoded),
            std::mem::discriminant(&variant)
        );
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
    for invalid in [
        json!({}),
        json!({"kind": "other"}),
        json!({"kind": ["plain"]}),
    ] {
        assert!(serde_json::from_value::<FormatChoice>(invalid).is_err());
    }
}

#[test]
fn identifiers_are_allocated_independently_of_the_wire_label() {
    let mut cases = std::collections::HashSet::new();
    // Rust compares the final two identifiers after Unicode NFC, but their
    // discriminator values must not be normalized.
    for label in ["a-b", "a_b", "self", "2nd", "\u{212b}", "\u{00c5}"] {
        let wire = json!({"kind": label});
        let decoded: IdentifierChoice = serde_json::from_value(wire.clone()).unwrap();
        assert!(cases.insert(std::mem::discriminant(&decoded)), "{label}");
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
}

#[test]
fn repeated_wrapper_keys_fall_back_to_payload_selection() {
    for wire in [
        json!({"item": {"count": 1}}),
        json!({"item": {"label": "x"}}),
    ] {
        let decoded: SharedWrapper = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
    for label in ["plain", "structured"] {
        let wire = json!({"kind": label});
        let decoded: ReferencedValues = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
}

#[test]
fn distinct_external_wrappers_keep_their_keys_and_payloads() {
    for wire in [
        json!({"sized": {"count": 1}}),
        json!({"note": {"label": "x"}}),
    ] {
        let decoded: DistinctWrappers = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
}
