use rust_tagged_wire_boundaries_raw::models::{Choice as RawChoice, Envelope, RequiredEnvelope};
use rust_tagged_wire_boundaries_strict::models::{
    Choice, ComposedChoice, LiteralChoice, Outer, ProjectedChoice, Sample, WithHolder,
};
use serde_json::{Value, json};

fn round_trip<T: serde::de::DeserializeOwned + serde::Serialize>(wire: Value) {
    let decoded = serde_json::from_value::<T>(wire.clone())
        .unwrap_or_else(|error| panic!("could not decode {wire}: {error}"));
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
}

#[test]
fn generated_tag_types_remain_distinct_when_a_schema_occupies_a_sibling_stem() {
    for tag in ["tag_1", "tag_12"] {
        round_trip::<Sample>(json!({"kind": tag, "label": "test"}));
    }
}

#[test]
fn scalar_and_array_alternatives_do_not_require_an_object_discriminator() {
    round_trip::<Choice>(json!("legacy"));
    round_trip::<Choice>(json!([1, 2]));
}

#[test]
fn selected_payload_keeps_nested_required_nulls() {
    for value in [Value::Null, json!(2)] {
        let wire = json!({
            "kind": "structured", "detail": {"value": value},
            "history": [{"value": null}],
            "indexed": {"key": {"value": null}},
        });
        let model: Choice = serde_json::from_value(wire.clone()).unwrap();
        let Choice::Structured { payload, .. } = &model else {
            panic!("expected the typed payload");
        };
        assert_eq!(payload.detail.value, value.as_i64());
        assert_eq!(payload.detail.note, None);
        assert_eq!(
            serde_json::to_value(&payload.detail).unwrap(),
            if value.is_null() {
                json!({})
            } else {
                json!({"value": value})
            }
        );
        assert_eq!(serde_json::to_value(model).unwrap(), wire);
    }
}

#[test]
fn flattened_payload_restores_required_nulls_in_declared_and_additional_values() {
    let wire = json!({
        "kind": "combined_alt", "name": "sample",
        "entries": {"nested": {"value": null}},
        "annotated": {"title": "heading", "pieces": [null, {"value": null}], "extra": {"value": null}},
    });
    let model: ComposedChoice = serde_json::from_value(wire.clone()).unwrap();
    let ComposedChoice::Combined { payload, .. } = &model else {
        panic!("expected the flattened typed payload");
    };
    assert_eq!(payload.name_part.name, "sample");
    assert_eq!(payload.index_part.annotated.title, "heading");
    assert_eq!(
        payload.index_part.annotated.additional_properties["extra"].value,
        None
    );
    assert_eq!(serde_json::to_value(model).unwrap(), wire);
    round_trip::<ComposedChoice>(json!({
        "kind": "combined", "name": "sample", "entries": null,
        "annotated": {"title": "heading"},
    }));
}

#[test]
fn whole_projected_union_keeps_the_declared_nested_null() {
    round_trip::<ProjectedChoice>(json!([{"value": null}]));
    round_trip::<ProjectedChoice>(json!(false));
}

#[test]
fn projected_payload_read_calls_original_codec_even_when_its_output_is_raw() {
    round_trip::<Outer>(json!({"scope": "s", "kind": "text_alt", "label": "a"}));
    round_trip::<WithHolder>(json!({"scope": "s", "inner": {"label": "a"}}));
    for error in [
        serde_json::from_value::<Outer>(json!({"scope": "s", "label": "a", "count": 1}))
            .unwrap_err(),
        serde_json::from_value::<WithHolder>(
            json!({"scope": "s", "inner": {"label": "a", "count": 1}}),
        )
        .unwrap_err(),
    ] {
        assert!(
            error
                .to_string()
                .contains("ambiguous inferred tagged union Inner"),
            "{error}"
        );
    }
}

#[test]
fn projected_payload_write_checks_nested_strict_on_assembled_wire() {
    let mut model: Outer = serde_json::from_value(json!({"scope": "s", "label": "a"})).unwrap();
    let Outer::Text { payload, .. } = &mut model else {
        panic!("expected the projected Text branch");
    };
    payload["count"] = json!(1);
    let error = serde_json::to_value(model).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous inferred tagged union Inner"),
        "{error}"
    );
}

#[test]
fn raw_model_fields_keep_presence_without_wrapping_other_optional_fields() {
    for wire in [
        json!({}),
        json!({"choice": null}),
        json!({"alias": null}),
        json!({"choice": [1]}),
    ] {
        let decoded: Envelope = serde_json::from_value(wire.clone()).unwrap();
        if wire.as_object().unwrap().contains_key("choice") && wire["choice"].is_null() {
            assert!(matches!(decoded.choice, Some(RawChoice::Raw(Value::Null))));
        }
        assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    }
    round_trip::<RequiredEnvelope>(json!({"choice": null}));
    assert!(serde_json::from_value::<RequiredEnvelope>(json!({})).is_err());
}

#[test]
fn finite_composite_literals_compare_numbers_without_rounding_or_ignoring_shape() {
    let marker = json!({"items": [1.0, 9007199254740993_u64, true]});
    round_trip::<LiteralChoice>(json!({"kind": "record_alt", "marker": marker}));
    for marker in [
        json!({"items": [1, 9007199254740992_u64, true]}),
        json!({"items": [1, 9007199254740993_u64, 1]}),
        json!({"items": [1, 9007199254740993_u64, true], "extra": null}),
        json!({"items": [1, 9007199254740993_u64, true, null]}),
    ] {
        assert!(
            serde_json::from_value::<LiteralChoice>(json!({"kind": "record", "marker": marker}))
                .is_err()
        );
    }
}
