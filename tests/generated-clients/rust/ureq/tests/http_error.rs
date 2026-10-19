#[path = "../../support/incomplete_body_server.rs"]
mod incomplete_server;
#[path = "../../support/one_response_server.rs"]
mod server;

use incomplete_server::spawn_incomplete_success_body_server;
use server::spawn_one_response_server;

use typed_error_responses_api::apis::{CreateResourceError, ResourcesApi};
use typed_error_responses_api::models::CreateResourceRequest;
use typed_error_responses_api::runtime::client::Client;
use typed_error_responses_api::runtime::error::ApiCallError;

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}

#[allow(dead_code)]
fn propagate_uniformly(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ApiCallError> {
    api.create_resource(request)?;
    api.check_no_success_body()?;
    Ok(())
}

#[test]
fn http_503_returns_operation_error_with_typed_body() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .expect_err("HTTP 503 must not be returned as transport or success");
    assert_eq!(err.retry_after_header(), Some("120"));

    match err {
        CreateResourceError::ServerError(api_error) => {
            assert_eq!(api_error.status_code(), 503);
            assert_eq!(api_error.raw_body(), body.as_bytes());
            assert_eq!(
                api_error
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok()),
                Some("120")
            );
            assert_eq!(
                api_error
                    .headers()
                    .get("x-trace-id")
                    .and_then(|value| value.to_str().ok()),
                Some("typed-error-test")
            );
            let typed_body = api_error
                .body()
                .expect("documented 5XX error body should decode");
            assert_eq!(typed_body.message, "temporarily unavailable");
            assert!(typed_body.retryable);
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[test]
fn operation_error_converts_to_uniform_api_call_error() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let error: ApiCallError = api
        .create_resource(&request)
        .expect_err("HTTP 503 must be returned as an operation error")
        .into();

    assert_eq!(error.operation_id(), "createResource");
    assert_eq!(error.status_code(), Some(503));
    assert_eq!(error.raw_body(), Some(body.as_bytes()));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("retry-after"))
            .and_then(|value| value.to_str().ok()),
        Some("120")
    );
}

#[test]
fn success_without_documented_body_does_not_drain_body() {
    let base_url = spawn_incomplete_success_body_server();
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);

    let response = api
        .check_no_success_body()
        .expect("success without a documented body must not read an incomplete body");

    assert_eq!(response.status_code, 200);
    assert_eq!(response.x_request_id_header(), Some("typed-response-test"));
    assert_eq!(
        response
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("typed-response-test")
    );
}
