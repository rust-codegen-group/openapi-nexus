// Compiled inside each freshly generated Rust client crate.

use nested_nullable_api::apis::{GetValuesError, GetValuesResponse, ValuesApi};
use nested_nullable_api::runtime::client::Client;
use nested_nullable_api::runtime::error::{ApiCallError, ApiError, Error};

fn check_metadata<T>(error: &ApiError<T>, status: u16, body: &str) {
    assert_eq!(error.status_code(), status);
    assert_eq!(error.raw_body(), body.as_bytes());
    assert_eq!(
        error.headers().get("x-trace-id").unwrap().to_str().unwrap(),
        "typed-error-test"
    );
}

#[tokio::test]
async fn map_values_decode_both_arrays_and_null() {
    let body = r#"{"present":["detail"],"absent":null}"#;
    let error = request("400 Bad Request", body)
        .await
        .expect_err("400 is an operation error");
    let GetValuesError::BadRequest(error) = error else {
        panic!("wrong HTTP error variant")
    };
    check_metadata(&error, 400, body);
    let decoded = error.body().expect("nullable map value decodes");
    assert_eq!(decoded["present"], Some(vec!["detail".to_string()]));
    assert_eq!(decoded["absent"], None);
}

#[tokio::test]
async fn array_items_decode_both_maps_and_null() {
    let body = r#"[null,{"detail":"message"}]"#;
    let error = request("401 Unauthorized", body)
        .await
        .expect_err("401 is an operation error");
    let GetValuesError::Unauthorized(error) = error else {
        panic!("wrong HTTP error variant")
    };
    check_metadata(&error, 401, body);
    let decoded = error.body().expect("nullable map item decodes");
    assert_eq!(decoded[0], None);
    assert_eq!(decoded[1].as_ref().unwrap()["detail"], "message");
}

#[tokio::test]
async fn nullable_container_is_not_confused_with_nullable_items() {
    for body in ["null", r#"["detail"]"#] {
        let error = request("402 Payment Required", body)
            .await
            .expect_err("402 is an operation error");
        let GetValuesError::Status402(error) = error else {
            panic!("wrong HTTP error variant")
        };
        check_metadata(&error, 402, body);
        let expected = if body == "null" {
            None
        } else {
            Some(vec!["detail".to_string()])
        };
        assert_eq!(error.body().expect("nullable array decodes"), &expected);
    }
}

fn check_decode_failure<T>(error: &ApiError<T>) {
    assert!(matches!(error.body(), Err(Error::Deserialize(_))));
}

#[tokio::test]
async fn invalid_payloads_keep_the_http_error_headers_and_raw_bytes() {
    for (status, body, code) in [
        ("400 Bad Request", r#"{"detail":[42]}"#, 400),
        ("400 Bad Request", r#"{"detail":[null]}"#, 400),
        ("401 Unauthorized", r#"[{"detail":null}]"#, 401),
        ("402 Payment Required", "[null]", 402),
        ("403 Forbidden", r#"{"detail":null}"#, 403),
        ("404 Not Found", r#"{"message":42}"#, 404),
        ("410 Gone", r#"[{"message":null}]"#, 410),
    ] {
        let error = request(status, body)
            .await
            .expect_err("invalid body is still an HTTP error");
        match &error {
            GetValuesError::BadRequest(error) => check_decode_failure(error),
            GetValuesError::Unauthorized(error) => check_decode_failure(error),
            GetValuesError::Status402(error) => check_decode_failure(error),
            GetValuesError::Forbidden(error) => check_decode_failure(error),
            GetValuesError::NotFound(error) => check_decode_failure(error),
            GetValuesError::Status410(error) => check_decode_failure(error),
            other => panic!("unexpected error: {other:?}"),
        }
        let uniform: ApiCallError = error.into();
        assert_eq!(uniform.operation_id(), "getValues");
        assert_eq!(uniform.status_code(), Some(code));
        assert_eq!(uniform.raw_body(), Some(body.as_bytes()));
        assert_eq!(
            uniform
                .headers()
                .unwrap()
                .get("x-trace-id")
                .unwrap()
                .to_str()
                .unwrap(),
            "typed-error-test"
        );
    }
}

#[tokio::test]
async fn model_aliases_decode_alone_and_inside_arrays() {
    let error = request("404 Not Found", r#"{"message":"missing"}"#)
        .await
        .expect_err("404 is an operation error");
    let GetValuesError::NotFound(error) = error else {
        panic!("wrong HTTP error variant")
    };
    assert_eq!(
        error.body().expect("model alias decodes").message,
        "missing"
    );

    let error = request("410 Gone", r#"[{"message":"expired"}]"#)
        .await
        .expect_err("410 is an operation error");
    let GetValuesError::Status410(error) = error else {
        panic!("wrong HTTP error variant")
    };
    assert_eq!(
        error.body().expect("array of model aliases decodes")[0].message,
        "expired"
    );
}

#[tokio::test]
async fn non_nullable_values_still_decode_without_optional_wrappers() {
    let body = r#"{"detail":["message"]}"#;
    let error = request("403 Forbidden", body)
        .await
        .expect_err("403 is an operation error");
    let GetValuesError::Forbidden(error) = error else {
        panic!("wrong HTTP error variant")
    };
    assert_eq!(
        error.body().expect("strict payload decodes")["detail"],
        vec!["message"]
    );
}

#[tokio::test]
async fn success_models_keep_nested_nullability() {
    let body = r#"{"mapValues":{"detail":null},"arrayItems":[null,"detail"],"arrayMaps":[null,{"detail":"message"}],"nullableArray":null,"strictValues":{"detail":["message"]}}"#;
    let response = request("200 OK", body)
        .await
        .expect("success model decodes");
    let model = response.data.expect("success payload exists");
    assert_eq!(model.map_values["detail"], None);
    assert_eq!(model.array_items, vec![None, Some("detail".to_string())]);
    assert_eq!(model.array_maps[0], None);
    assert_eq!(model.array_maps[1].as_ref().unwrap()["detail"], "message");
    assert_eq!(model.nullable_array, None);
    assert_eq!(model.strict_values["detail"], vec!["message"]);
}
