use rust_union_string_const::models::{FormatChoice, SingleChoice};
use serde_json::{Value, json};

#[test]
fn a_single_alternative_still_requires_its_literal() {
    let wire = json!({"kind": "plain"});
    let decoded: SingleChoice = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    assert!(serde_json::from_value::<SingleChoice>(json!({"kind": "other"})).is_err());
}

#[test]
fn authored_string_constants_select_distinct_ordinary_union_members() {
    let plain = json!({"kind": "plain"});
    let structured = json!({"kind": "structured"});
    let to_wire = |wire: Value, first: bool| {
        let value: FormatChoice = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            matches!(&value, FormatChoice::FormatChoiceMember1(_)),
            first
        );
        assert_eq!(
            matches!(&value, FormatChoice::FormatChoiceMember3(_)),
            !first
        );
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    };
    to_wire(plain, true);
    to_wire(structured, false);

    let wire = json!({"count": 3});
    let decoded: FormatChoice = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(&decoded, FormatChoice::FormatChoiceMember2(_)));
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);

    for wire in [
        json!({"kind":"other"}),
        json!({}),
        json!({"kind":7}),
        json!({"kind":null}),
    ] {
        assert!(
            serde_json::from_value::<FormatChoice>(wire.clone()).is_err(),
            "accepted {wire}"
        );
    }
}
