#[path = "../../support/incomplete_body_server.rs"]
mod incomplete_server;
#[path = "../../support/one_response_server.rs"]
mod server;

use incomplete_server::spawn_incomplete_success_body_server;
use server::spawn_one_response_server;

use typed_error_responses_api::apis::{
    CreateResourceError, CreateResourceWithWildcardSuccessError, ResourcesApi,
};
use typed_error_responses_api::models::CreateResourceRequest;
use typed_error_responses_api::runtime::client::Client;
use typed_error_responses_api::runtime::error::{ApiCallError, Error};

#[test]
fn api_call_error_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<ApiCallError>(),
        std::mem::size_of::<Box<()>>()
    );
}

#[allow(dead_code)]
async fn propagate_uniformly(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ApiCallError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("SDK request failed: {source}")]
struct ThisErrorAppError {
    #[source]
    source: ApiCallError,
}

impl<E> From<E> for ThisErrorAppError
where
    E: Into<ApiCallError>,
{
    fn from(error: E) -> Self {
        Self {
            source: error.into(),
        }
    }
}

#[allow(dead_code)]
async fn propagate_with_thiserror(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), ThisErrorAppError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[derive(Debug, snafu::Snafu)]
#[snafu(context(false))]
struct SnafuAppError {
    #[snafu(source(from(generic)))]
    source: ApiCallError,
}

#[allow(dead_code)]
async fn propagate_with_snafu(
    api: &ResourcesApi<'_>,
    request: &CreateResourceRequest,
) -> Result<(), SnafuAppError> {
    api.create_resource(request).await?;
    api.check_no_success_body().await?;
    Ok(())
}

#[tokio::test]
async fn http_503_returns_operation_error_with_typed_body() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .await
        .expect_err("HTTP 503 must not be returned as success");
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
                .expect("documented 5XX error body should decode lazily");
            assert_eq!(typed_body.message, "temporarily unavailable");
            assert!(typed_body.retryable);
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_error_body_keeps_http_error_and_raw_body() {
    let body = r#"{"message":42,"retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource(&request)
        .await
        .expect_err("HTTP 503 must remain an operation error even when its body is invalid");

    match err {
        CreateResourceError::ServerError(api_error) => {
            assert_eq!(api_error.status_code(), 503);
            assert_eq!(api_error.raw_body(), body.as_bytes());
            assert!(matches!(api_error.body(), Err(Error::Deserialize(_))));

            let error: ApiCallError = CreateResourceError::ServerError(api_error).into();
            assert_eq!(error.operation_id(), "createResource");
            assert_eq!(error.status_code(), Some(503));
            assert_eq!(error.raw_body(), Some(body.as_bytes()));
            assert!(matches!(
                std::error::Error::source(&error).and_then(|source| source.downcast_ref::<Error>()),
                Some(Error::Deserialize(_))
            ));
        }
        other => panic!("expected ServerError variant, got {other:?}"),
    }
}

#[tokio::test]
async fn operation_error_converts_to_uniform_api_call_error() {
    let body = r#"{"message":"temporarily unavailable","retryable":true}"#;
    let base_url = spawn_one_response_server("503 Service Unavailable", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let error: ApiCallError = api
        .create_resource(&request)
        .await
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
    assert_eq!(
        error.to_string(),
        "operation createResource failed with HTTP status 503"
    );
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn transport_error_converts_to_uniform_api_call_error() {
    let error: ApiCallError =
        CreateResourceError::Transport(Error::Unsupported("test transport failure")).into();

    assert_eq!(error.operation_id(), "createResource");
    assert_eq!(error.status_code(), None);
    assert!(error.headers().is_none());
    assert!(error.raw_body().is_none());
    assert!(matches!(
        std::error::Error::source(&error).and_then(|source| source.downcast_ref::<Error>()),
        Some(Error::Unsupported("test transport failure"))
    ));
}

#[tokio::test]
async fn exact_success_status_wins_over_2xx_wildcard() {
    let body = r#"{"id":"exact-created"}"#;
    let base_url = spawn_one_response_server("201 Created", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let response = api
        .create_resource_with_wildcard_success(&request)
        .await
        .expect("HTTP 201 should be a successful exact response");

    let created = response
        .created
        .expect("201 must match the exact branch before 2XX wildcard");
    assert_eq!(created.id, "exact-created");
    assert!(response.status_2xx.is_none());
    assert_eq!(
        response
            .headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("typed-response-test")
    );
}

#[tokio::test]
async fn unexpected_non_success_status_uses_unexpected_detail() {
    let body = r#"{"message":"teapot"}"#;
    let base_url = spawn_one_response_server("418 I'm a teapot", body);
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);
    let request = CreateResourceRequest {
        name: "resource".to_string(),
    };

    let err = api
        .create_resource_with_wildcard_success(&request)
        .await
        .expect_err("undocumented HTTP 418 must not be returned as success");

    match err {
        CreateResourceWithWildcardSuccessError::Unexpected(api_error) => {
            assert_eq!(api_error.status_code(), 418);
            assert_eq!(api_error.raw_body(), body.as_bytes());
        }
        other => panic!("expected Unexpected variant, got {other:?}"),
    }
}

#[tokio::test]
async fn success_without_documented_body_does_not_drain_body() {
    let base_url = spawn_incomplete_success_body_server();
    let client = Client::new(&base_url);
    let api = ResourcesApi::new(&client);

    let response = api
        .check_no_success_body()
        .await
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
