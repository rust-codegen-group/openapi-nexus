use serde_json::{Value, json};
use union_selection_contract::models::{
    Choice, ClientTool, Envelope, NumericTwo, Request, RequiredChoice, SearchTool, Tool,
};

fn client_wire() -> Value {
    json!({"name": "weather", "input_schema": {"type": "object"}})
}

fn search_wire() -> Value {
    json!({"type": "web_search_20250305", "name": "web_search", "max_uses": 1})
}

fn numeric_wire() -> Value {
    json!({"revision": 2, "second_value": "two"})
}

#[test]
fn direct_tool_members_preserve_wire() {
    let client: ClientTool = serde_json::from_value(client_wire()).unwrap();
    let search: SearchTool = serde_json::from_value(search_wire()).unwrap();
    assert_eq!(serde_json::to_value(client).unwrap(), client_wire());
    assert_eq!(serde_json::to_value(search).unwrap(), search_wire());
}

#[test]
fn typed_union_request_is_json_serializable() {
    let request = Request {
        tools: vec![
            Tool::ClientTool(serde_json::from_value(client_wire()).unwrap()),
            Tool::SearchTool(serde_json::from_value(search_wire()).unwrap()),
        ],
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"tools": [client_wire(), search_wire()]})
    );
}

#[test]
fn inbound_tool_request_constructs_union_variants() {
    let wire = json!({"tools": [client_wire(), search_wire()]});
    let request: Request = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(request.tools[0], Tool::ClientTool(_)));
    assert!(matches!(request.tools[1], Tool::SearchTool(_)));
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
