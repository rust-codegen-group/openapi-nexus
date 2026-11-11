use super::request;
use rust_union_first_match::apis::GetChoiceError;
use rust_union_first_match::models::FiniteChoice;
use rust_union_first_match::runtime::error::{ApiCallError, Error};

const BODY: &str =
    r#"{"revision":2,"values":[2.5],"flags":{"keep_key":true},"second_value":"two"}"#;

#[tokio::test]
async fn success_response_selects_the_finite_member_without_losing_its_payload() {
    let response = request("200 OK", BODY).await.unwrap();
    let value = response.data.unwrap();
    assert!(matches!(value, FiniteChoice::FiniteTwo(_)));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::from_str::<serde_json::Value>(BODY).unwrap()
    );
}

#[tokio::test]
async fn error_response_caches_the_same_correctly_selected_member() {
    let error = request("400 Bad Request", BODY).await.unwrap_err();
    let GetChoiceError::BadRequest(error) = error else {
        panic!("typed 400");
    };
    let first = error.body().unwrap();
    let second = error.body().unwrap();
    assert!(std::ptr::eq(first, second));
    assert!(matches!(first, FiniteChoice::FiniteTwo(_)));
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::from_str::<serde_json::Value>(BODY).unwrap()
    );
    assert_eq!(error.status_code(), 400);
    assert_eq!(error.raw_body(), BODY.as_bytes());
    assert_eq!(
        error.headers().get("x-trace-id").unwrap().to_str().unwrap(),
        "typed-error-test"
    );
}

#[tokio::test]
async fn unmatched_error_payload_preserves_raw_http_metadata_and_decode_failure() {
    let body = r#"{"revision":3,"values":[2.5],"flags":{"keep_key":true}}"#;
    let error = request("400 Bad Request", body).await.unwrap_err();
    let GetChoiceError::BadRequest(ref typed) = error else {
        panic!("typed 400");
    };
    assert!(matches!(typed.body(), Err(Error::Deserialize(_))));
    let uniform: ApiCallError = error.into();
    assert_eq!(uniform.operation_id(), "getChoice");
    assert_eq!(uniform.status_code(), Some(400));
    assert_eq!(uniform.raw_body(), Some(body.as_bytes()));
}
