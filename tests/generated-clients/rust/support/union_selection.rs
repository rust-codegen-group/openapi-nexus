use serde_json::{Value, json};
use union_selection_contract::models::{
    Choice, Envelope, NumericTwo, PayloadA, PayloadB, RequiredChoice, VariantRequest, VariantSet,
};

fn payload_a_wire() -> Value {
    json!({"label": "weather", "payload_schema": {"type": "object"}})
}

fn payload_b_wire() -> Value {
    json!({"kind": "variant_b", "label": "payload_b", "limit": 1})
}

fn numeric_wire() -> Value {
    json!({"revision": 2, "second_value": "two"})
}

#[test]
fn direct_variant_members_preserve_wire() {
    let payload_a: PayloadA = serde_json::from_value(payload_a_wire()).unwrap();
    let payload_b: PayloadB = serde_json::from_value(payload_b_wire()).unwrap();
    assert_eq!(serde_json::to_value(payload_a).unwrap(), payload_a_wire());
    assert_eq!(serde_json::to_value(payload_b).unwrap(), payload_b_wire());
}

#[test]
fn typed_variant_request_is_json_serializable() {
    let request = VariantRequest {
        choices: vec![
            VariantSet::PayloadA(serde_json::from_value(payload_a_wire()).unwrap()),
            VariantSet::PayloadB(serde_json::from_value(payload_b_wire()).unwrap()),
        ],
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"choices": [payload_a_wire(), payload_b_wire()]})
    );
}

#[test]
fn inbound_variant_request_constructs_union_variants() {
    let wire = json!({"choices": [payload_a_wire(), payload_b_wire()]});
    let request: VariantRequest = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(request.choices[0], VariantSet::PayloadA(_)));
    assert!(matches!(request.choices[1], VariantSet::PayloadB(_)));
    assert_eq!(serde_json::to_value(request).unwrap(), wire);
}

#[test]
fn direct_numeric_member_preserves_payload() {
    let value: NumericTwo = serde_json::from_value(numeric_wire()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), numeric_wire());
}

#[test]
fn disjoint_required_fields_select_second_member() {
    let value: RequiredChoice = serde_json::from_value(numeric_wire()).unwrap();
    assert!(matches!(value, RequiredChoice::RequiredTwo(_)));
    assert_eq!(serde_json::to_value(value).unwrap(), numeric_wire());
}

#[test]
fn disjoint_numeric_enums_select_second_member() {
    let value: Choice = serde_json::from_value(numeric_wire()).unwrap();
    assert!(matches!(value, Choice::NumericTwo(_)));
    assert_eq!(serde_json::to_value(value).unwrap(), numeric_wire());
}

#[test]
fn nested_scalar_array_map_unions_preserve_payloads() {
    let wire = json!({"choice": numeric_wire(), "choice_list": [numeric_wire()], "choice_map": {"keep_key": numeric_wire()}});
    let value: Envelope = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(value.choice, Choice::NumericTwo(_)));
    assert!(matches!(value.choice_list[0], Choice::NumericTwo(_)));
    assert!(matches!(
        value.choice_map["keep_key"],
        Choice::NumericTwo(_)
    ));
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
}
